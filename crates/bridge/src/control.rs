// SPDX-License-Identifier: Apache-2.0

//! Stream Control state machine (`PROTOCOL.md` §12).
//!
//! Holds the currently-requested stream configuration and applies incoming
//! [`ControlRequest`]s to it. The device may lower a requested rate or refuse an
//! unsupported stream, but must never silently raise a rate; the effective
//! values it actually applied come back in the [`ControlResponse`]. This type
//! encodes those clamping rules once, so the application just calls
//! [`StreamState::apply`] and forwards the response.

use snf_ble::protocol::{
    ControlOp, ControlRequest, ControlResponse, ControlResult, SUBJECT_UNKNOWN, StreamSettings,
    capabilities, streams,
};

/// Allowed vitals rate, Hz (`PROTOCOL.md` §7).
pub const VITALS_HZ_RANGE: (u8, u8) = (1, 10);
/// Allowed pose rate, Hz (`PROTOCOL.md` §9).
pub const POSE_HZ_RANGE: (u8, u8) = (1, 20);
/// Allowed point-cloud rate, Hz (`PROTOCOL.md` §10).
pub const POINT_CLOUD_HZ_RANGE: (u8, u8) = (1, 10);

/// The device's requested/active stream configuration.
///
/// [`Default`] is the post-connect baseline from `PROTOCOL.md` §13: Status and
/// Vitals (and Fatigue) on, Vitals at 2 Hz, Pose and Point Cloud off but with
/// their recommended rates staged for when a client enables them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamState {
    pub stream_mask: u16,
    pub vitals_hz: u8,
    pub pose_hz: u8,
    pub point_cloud_hz: u8,
    pub max_points: u8,
    /// Pinned tracking subject, or `0xffff` for auto-select.
    pub subject_id: u16,
}

impl Default for StreamState {
    fn default() -> Self {
        Self {
            stream_mask: streams::STATUS | streams::VITALS | streams::FATIGUE,
            vitals_hz: 2,
            pose_hz: 10,
            point_cloud_hz: 5,
            max_points: 96,
            subject_id: 0xffff,
        }
    }
}

impl StreamState {
    /// Which stream bits this build can actually serve, given its capabilities.
    /// Status is always available; the rest gate on the matching capability bit.
    fn allowed_mask(capabilities_bits: u32) -> u16 {
        let mut mask = streams::STATUS;
        if capabilities_bits & capabilities::VITALS != 0 {
            mask |= streams::VITALS;
        }
        if capabilities_bits & capabilities::FATIGUE != 0 {
            mask |= streams::FATIGUE;
        }
        if capabilities_bits & capabilities::POSE_3D != 0 {
            mask |= streams::POSE;
        }
        if capabilities_bits & capabilities::POINT_CLOUD_3D != 0 {
            mask |= streams::POINT_CLOUD;
        }
        mask
    }

    /// Apply a request, mutating state where appropriate, and return the
    /// response to indicate back to the client.
    ///
    /// `capabilities_bits` and `max_points_cap` come from this build's Protocol
    /// Info: streams the device cannot serve are masked out, and `max_points` is
    /// capped at what the device advertises.
    ///
    /// The result codes mirror the ESP32-C5 firmware exactly (`PROTOCOL.md`
    /// §12): a malformed body is `Invalid` and changes nothing, a request naming
    /// a stream this build cannot serve applies the supported subset and reports
    /// `Unsupported`, and everything else is `Success`. A client must be able to
    /// interpret a response the same way regardless of which device answered.
    pub fn apply(
        &mut self,
        request: &ControlRequest,
        capabilities_bits: u32,
        max_points_cap: u8,
    ) -> ControlResponse {
        let result = match &request.op {
            ControlOp::SetStreams(settings) => {
                self.apply_set_streams(settings, capabilities_bits, max_points_cap)
            }
            ControlOp::SetSubject(subject_id) => {
                self.apply_set_subject(*subject_id, capabilities_bits)
            }
            // The snapshot itself is performed by the application; here we only
            // report whether every requested stream can actually be served.
            ControlOp::RequestSnapshot(mask) => {
                unsupported_if_extra(*mask, Self::allowed_mask(capabilities_bits))
            }
            ControlOp::Ping(echo) => {
                return ControlResponse {
                    request_id: request.request_id,
                    opcode: request.opcode,
                    result: ControlResult::Success,
                    ..self.response_body(echo.clone())
                };
            }
            ControlOp::Unsupported => ControlResult::Unsupported,
            ControlOp::Invalid => ControlResult::Invalid,
        };

        ControlResponse {
            request_id: request.request_id,
            opcode: request.opcode,
            result,
            ..self.response_body(Vec::new())
        }
    }

    /// `SET_STREAMS`: validate every rate first, then apply.
    ///
    /// Validation strictly precedes mutation so a rejected request leaves the
    /// previous configuration intact — a half-applied `SET_STREAMS` would leave
    /// the client and device disagreeing about what is running, and the two
    /// implementations would disagree about where the split falls.
    fn apply_set_streams(
        &mut self,
        settings: &StreamSettings,
        capabilities_bits: u32,
        max_points_cap: u8,
    ) -> ControlResult {
        // `vitals_hz` is always validated, even when the vitals bit is clear —
        // the ESP32-C5 does the same, so both reject the same bytes.
        if !in_range(settings.vitals_hz, VITALS_HZ_RANGE) {
            return ControlResult::Invalid;
        }
        // Pose and point-cloud rates are validated only when their stream is
        // being enabled; `0` elsewhere means "leave the staged rate alone".
        if settings.stream_mask & streams::POSE != 0 && !in_range(settings.pose_hz, POSE_HZ_RANGE) {
            return ControlResult::Invalid;
        }
        if settings.stream_mask & streams::POINT_CLOUD != 0
            && !in_range(settings.point_cloud_hz, POINT_CLOUD_HZ_RANGE)
        {
            return ControlResult::Invalid;
        }

        let allowed = Self::allowed_mask(capabilities_bits);
        self.stream_mask = settings.stream_mask & allowed;
        self.vitals_hz = settings.vitals_hz;
        if settings.pose_hz != 0 {
            self.pose_hz = settings.pose_hz;
        }
        if settings.point_cloud_hz != 0 {
            self.point_cloud_hz = settings.point_cloud_hz;
        }
        if settings.max_points != 0 {
            self.max_points = settings.max_points.min(max_points_cap);
        }
        unsupported_if_extra(settings.stream_mask, allowed)
    }

    /// `SET_SUBJECT`: only auto-select is honoured unless this build advertises
    /// `MULTI_SUBJECT`. Pinning a subject the device cannot track is reported
    /// `Unsupported` rather than silently accepted, so a client is never told a
    /// pin succeeded when the device kept auto-selecting.
    fn apply_set_subject(&mut self, subject_id: u16, capabilities_bits: u32) -> ControlResult {
        if subject_id != SUBJECT_UNKNOWN && capabilities_bits & capabilities::MULTI_SUBJECT == 0 {
            return ControlResult::Unsupported;
        }
        self.subject_id = subject_id;
        ControlResult::Success
    }

    /// Fill the `effective_*` fields of a response from current state.
    fn response_body(&self, echo: Vec<u8>) -> ControlResponse {
        ControlResponse {
            request_id: 0,
            opcode: 0,
            result: ControlResult::Success,
            effective_stream_mask: self.stream_mask,
            effective_vitals_hz: self.vitals_hz,
            effective_pose_hz: self.pose_hz,
            effective_point_cloud_hz: self.point_cloud_hz,
            effective_max_points: self.max_points,
            echo,
        }
    }
}

/// Whether a requested rate falls inside `[min, max]`.
fn in_range(requested: u8, range: (u8, u8)) -> bool {
    requested >= range.0 && requested <= range.1
}

/// `Unsupported` if `requested` names any stream outside `allowed`, else
/// `Success`. The supported subset is still applied by the caller — the code
/// reports that part of the request was dropped, it does not reject it.
fn unsupported_if_extra(requested: u16, allowed: u16) -> ControlResult {
    if requested & !allowed != 0 {
        ControlResult::Unsupported
    } else {
        ControlResult::Success
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_streams(request_id: u16, settings: StreamSettings) -> ControlRequest {
        ControlRequest {
            request_id,
            opcode: 0x01,
            op: ControlOp::SetStreams(settings),
        }
    }

    #[test]
    fn default_matches_protocol_baseline() {
        let state = StreamState::default();
        assert_eq!(
            state.stream_mask,
            streams::STATUS | streams::VITALS | streams::FATIGUE
        );
        assert_eq!(state.vitals_hz, 2);
    }

    /// Asking for a stream this build cannot serve applies the supported subset
    /// and reports `Unsupported` — the ESP32-C5's exact behaviour.
    #[test]
    fn masks_out_unsupported_streams_and_caps_points() {
        let mut state = StreamState::default();
        // Client asks for pose + point cloud, but this build only has vitals.
        let request = set_streams(
            1,
            StreamSettings {
                stream_mask: streams::VITALS | streams::POSE | streams::POINT_CLOUD,
                vitals_hz: 5,
                pose_hz: 10,
                point_cloud_hz: 5,
                max_points: 200,
            },
        );
        let response = state.apply(&request, capabilities::VITALS, 96);
        assert_eq!(response.result, ControlResult::Unsupported);
        // Pose / point-cloud bits dropped; vitals kept.
        assert_eq!(response.effective_stream_mask, streams::VITALS);
        assert_eq!(response.effective_vitals_hz, 5);
        assert_eq!(response.effective_max_points, 96); // capped
        assert_eq!(response.opcode, 0x01);

        // Every requested stream servable => Success.
        let request = set_streams(
            2,
            StreamSettings {
                stream_mask: streams::STATUS | streams::VITALS,
                vitals_hz: 2,
                pose_hz: 0,
                point_cloud_hz: 0,
                max_points: 0,
            },
        );
        let response = state.apply(&request, capabilities::VITALS, 96);
        assert_eq!(response.result, ControlResult::Success);
    }

    /// An out-of-range rate is refused outright and changes nothing, matching
    /// the ESP32-C5's `1..=10` check on `vitals_hz`.
    #[test]
    fn rejects_out_of_range_rate_without_applying() {
        let mut state = StreamState::default();
        let before = state;
        for bad_hz in [0u8, 11, 250] {
            let request = set_streams(
                3,
                StreamSettings {
                    stream_mask: streams::VITALS,
                    vitals_hz: bad_hz,
                    pose_hz: 0,
                    point_cloud_hz: 0,
                    max_points: 0,
                },
            );
            let response = state.apply(&request, capabilities::VITALS, 96);
            assert_eq!(response.result, ControlResult::Invalid, "hz={bad_hz}");
            assert_eq!(state, before, "rejected request must not half-apply");
        }
        assert_eq!(VITALS_HZ_RANGE, (1, 10));
    }

    /// A zero pose / point-cloud rate leaves the staged value alone; a bad one
    /// is only rejected when that stream is actually being enabled.
    #[test]
    fn zero_rates_leave_staged_values_untouched() {
        let mut state = StreamState::default();
        let staged_pose = state.pose_hz;
        let request = set_streams(
            4,
            StreamSettings {
                stream_mask: streams::VITALS,
                vitals_hz: 4,
                pose_hz: 0,
                point_cloud_hz: 0,
                max_points: 0,
            },
        );
        let response = state.apply(&request, capabilities::VITALS, 96);
        assert_eq!(response.result, ControlResult::Success);
        assert_eq!(state.pose_hz, staged_pose);
        assert_eq!(state.max_points, StreamState::default().max_points);

        // Enabling pose at an impossible rate is Invalid.
        let request = set_streams(
            5,
            StreamSettings {
                stream_mask: streams::POSE,
                vitals_hz: 2,
                pose_hz: 99,
                point_cloud_hz: 0,
                max_points: 0,
            },
        );
        let caps = capabilities::VITALS | capabilities::POSE_3D;
        assert_eq!(
            state.apply(&request, caps, 96).result,
            ControlResult::Invalid
        );
    }

    /// Pinning a subject on a single-subject build is `Unsupported`, not a
    /// silent success — the ESP32-C5 answers the same way.
    #[test]
    fn set_subject_rejects_a_pin_without_multi_subject() {
        let mut state = StreamState::default();
        let pin = ControlRequest {
            request_id: 6,
            opcode: 0x02,
            op: ControlOp::SetSubject(42),
        };
        let response = state.apply(&pin, capabilities::VITALS, 96);
        assert_eq!(response.opcode, 0x02);
        assert_eq!(response.result, ControlResult::Unsupported);
        assert_eq!(
            state.subject_id, SUBJECT_UNKNOWN,
            "pin must not take effect"
        );

        // Auto-select is always accepted.
        let auto = ControlRequest {
            request_id: 7,
            opcode: 0x02,
            op: ControlOp::SetSubject(SUBJECT_UNKNOWN),
        };
        assert_eq!(
            state.apply(&auto, capabilities::VITALS, 96).result,
            ControlResult::Success
        );

        // A multi-subject build honours the pin.
        let caps = capabilities::VITALS | capabilities::MULTI_SUBJECT;
        assert_eq!(state.apply(&pin, caps, 96).result, ControlResult::Success);
        assert_eq!(state.subject_id, 42);
    }

    #[test]
    fn ping_round_trips_and_snapshot_reports_unservable_streams() {
        let mut state = StreamState::default();
        let ping = ControlRequest {
            request_id: 8,
            opcode: 0x04,
            op: ControlOp::Ping(b"hi".to_vec()),
        };
        let response = state.apply(&ping, capabilities::VITALS, 96);
        assert_eq!(response.opcode, 0x04);
        assert_eq!(response.echo, b"hi");
        assert_eq!(response.result, ControlResult::Success);

        let snapshot = ControlRequest {
            request_id: 9,
            opcode: 0x03,
            op: ControlOp::RequestSnapshot(streams::VITALS | streams::POINT_CLOUD),
        };
        assert_eq!(
            state.apply(&snapshot, capabilities::VITALS, 96).result,
            ControlResult::Unsupported
        );
    }

    /// An unknown opcode and a malformed body are answered, never dropped: the
    /// client must not be left waiting for an indication that never comes.
    #[test]
    fn unparseable_ops_are_still_answered() {
        let mut state = StreamState::default();
        let unknown = ControlRequest {
            request_id: 10,
            opcode: 0x7F,
            op: ControlOp::Unsupported,
        };
        let response = state.apply(&unknown, capabilities::VITALS, 96);
        assert_eq!(
            response.opcode, 0x7F,
            "response echoes the opcode asked for"
        );
        assert_eq!(response.result, ControlResult::Unsupported);

        let malformed = ControlRequest {
            request_id: 11,
            opcode: 0x01,
            op: ControlOp::Invalid,
        };
        let response = state.apply(&malformed, capabilities::VITALS, 96);
        assert_eq!(response.result, ControlResult::Invalid);
        assert_eq!(state, StreamState::default(), "must not mutate state");
    }
}
