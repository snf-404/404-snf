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
    ControlOp, ControlRequest, ControlResponse, ControlResult, StreamSettings, capabilities,
    streams,
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
    pub fn apply(
        &mut self,
        request: &ControlRequest,
        capabilities_bits: u32,
        max_points_cap: u8,
    ) -> ControlResponse {
        let (opcode, result) = match &request.op {
            ControlOp::SetStreams(settings) => {
                self.apply_set_streams(settings, capabilities_bits, max_points_cap);
                (0x01, ControlResult::Success)
            }
            ControlOp::SetSubject(subject_id) => {
                self.subject_id = *subject_id;
                (0x02, ControlResult::Success)
            }
            // The snapshot itself is performed by the application; here we just
            // acknowledge and echo the current effective configuration.
            ControlOp::RequestSnapshot(_mask) => (0x03, ControlResult::Success),
            ControlOp::Ping(echo) => {
                return ControlResponse {
                    request_id: request.request_id,
                    opcode: 0x04,
                    result: ControlResult::Success,
                    ..self.response_body(echo.clone())
                };
            }
        };

        ControlResponse {
            request_id: request.request_id,
            opcode,
            result,
            ..self.response_body(Vec::new())
        }
    }

    fn apply_set_streams(
        &mut self,
        settings: &StreamSettings,
        capabilities_bits: u32,
        max_points_cap: u8,
    ) {
        // Refuse streams this build cannot serve (§12: no silent enable of the
        // unsupported), and clamp each rate into its allowed range.
        self.stream_mask = settings.stream_mask & Self::allowed_mask(capabilities_bits);
        self.vitals_hz = clamp_hz(settings.vitals_hz, VITALS_HZ_RANGE);
        self.pose_hz = clamp_hz(settings.pose_hz, POSE_HZ_RANGE);
        self.point_cloud_hz = clamp_hz(settings.point_cloud_hz, POINT_CLOUD_HZ_RANGE);
        self.max_points = settings.max_points.min(max_points_cap);
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

/// Clamp a requested rate into `[min, max]`. A request of `0` (a client
/// disabling via the mask) still clamps up to `min`; the mask, not the rate,
/// governs whether the stream runs.
fn clamp_hz(requested: u8, range: (u8, u8)) -> u8 {
    requested.clamp(range.0, range.1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_streams(request_id: u16, settings: StreamSettings) -> ControlRequest {
        ControlRequest {
            request_id,
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
        assert_eq!(response.result, ControlResult::Success);
        // Pose / point-cloud bits dropped; vitals kept.
        assert_eq!(response.effective_stream_mask, streams::VITALS);
        assert_eq!(response.effective_vitals_hz, 5);
        assert_eq!(response.effective_max_points, 96); // capped
        assert_eq!(response.opcode, 0x01);
    }

    #[test]
    fn clamps_rate_into_allowed_range() {
        let mut state = StreamState::default();
        let request = set_streams(
            2,
            StreamSettings {
                stream_mask: streams::VITALS,
                vitals_hz: 250, // way above max
                pose_hz: 0,
                point_cloud_hz: 0,
                max_points: 0,
            },
        );
        state.apply(&request, capabilities::VITALS, 96);
        assert_eq!(state.vitals_hz, VITALS_HZ_RANGE.1);
    }

    #[test]
    fn set_subject_and_ping_round_trip() {
        let mut state = StreamState::default();
        let set = ControlRequest {
            request_id: 3,
            op: ControlOp::SetSubject(42),
        };
        let response = state.apply(&set, capabilities::VITALS, 96);
        assert_eq!(response.opcode, 0x02);
        assert_eq!(state.subject_id, 42);

        let ping = ControlRequest {
            request_id: 4,
            op: ControlOp::Ping(b"hi".to_vec()),
        };
        let response = state.apply(&ping, capabilities::VITALS, 96);
        assert_eq!(response.opcode, 0x04);
        assert_eq!(response.echo, b"hi");
        assert_eq!(response.result, ControlResult::Success);
    }
}
