// SPDX-License-Identifier: Apache-2.0

//! Pure-Rust parsers for IWR6843 UART point-cloud protocols.

use std::{error::Error, fmt};

use serde::{Deserialize, Serialize};

/// Byte sequence that starts every IWR6843 UART packet.
pub const MAGIC_WORD: [u8; 8] = [0x02, 0x01, 0x04, 0x03, 0x06, 0x05, 0x08, 0x07];

pub(crate) const FRAME_HEADER_LEN: usize = 40;
const TLV_HEADER_LEN: usize = 8;
const POINT_LEN: usize = 16;
const SIDE_INFO_LEN: usize = 4;
const RANGE_PROFILE_BIN_LEN: usize = 2;
const PROCESSING_STATS_LEN: usize = 24;
const TEMPERATURE_STATS_LEN: usize = 28;

const DETECTED_POINTS: u32 = 1;
const RANGE_PROFILE: u32 = 2;
const PROCESSING_STATS: u32 = 6;
const DETECTED_POINTS_SIDE_INFO: u32 = 7;
const TEMPERATURE_STATS: u32 = 9;
#[cfg(feature = "vital-signs")]
const TRACKER_TARGET_LIST: u32 = 1010;
#[cfg(feature = "vital-signs")]
const TRACKER_TARGET_INDEX: u32 = 1011;
#[cfg(feature = "vital-signs")]
const COMPRESSED_SPHERICAL_POINTS: u32 = 1020;
#[cfg(feature = "vital-signs")]
const VITAL_SIGNS: u32 = 0x410;
#[cfg(feature = "vital-signs")]
const COMPRESSED_POINT_UNITS_LEN: usize = 20;
#[cfg(feature = "vital-signs")]
const COMPRESSED_POINT_LEN: usize = 8;
#[cfg(feature = "vital-signs")]
const VITAL_SIGNS_LEN: usize = 136;
/// `tid` + 9 state floats + a 4×4 covariance + gain + confidence.
#[cfg(feature = "vital-signs")]
const TRACKER_TARGET_LEN: usize = 112;
/// Track-index sentinels: the values above the largest track ID that TI uses to
/// say *why* a point was not associated rather than *with what*.
#[cfg(feature = "vital-signs")]
const TRACK_INDEX_WEAK_SNR: u8 = 253;
#[cfg(feature = "vital-signs")]
const TRACK_INDEX_OUTSIDE_BOUNDARY: u8 = 254;
#[cfg(feature = "vital-signs")]
const TRACK_INDEX_NOISE: u8 = 255;

/// UART payload protocol selected by the firmware flashed on the radar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RadarProtocol {
    /// Factory/out-of-box mmWave demo with Cartesian point TLVs. Retained for
    /// replaying older captures and for a sensor that has been flashed back;
    /// 404-snf's own sensors run the vital-signs firmware.
    #[cfg_attr(not(feature = "vital-signs"), default)]
    OutOfBox,
    /// Radar Toolbox Vital Signs With People Tracking demo — what this project
    /// deploys, and so the default whenever the feature is compiled in.
    #[cfg(feature = "vital-signs")]
    #[default]
    VitalSigns,
}

/// Fixed 40-byte header emitted by the xWR68xx demos.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FrameHeader {
    pub version: u32,
    pub packet_length: u32,
    pub platform: u32,
    pub frame_number: u32,
    pub time_cpu_cycles: u32,
    pub num_detected_objects: u32,
    pub num_tlvs: u32,
    pub subframe_number: u32,
}

/// One dot in a radar point-cloud graph.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RadarPoint {
    /// Cartesian position relative to the sensor, in metres.
    pub x: f32,
    /// Distance outward from the antenna plane, in metres.
    pub y: f32,
    /// Vertical position relative to the sensor, in metres.
    pub z: f32,
    /// Radial velocity away from the sensor, in metres per second.
    pub velocity: f32,
    /// Signal-to-noise ratio, in dB, when supplied by the firmware.
    pub snr_db: Option<f32>,
    /// Noise level, in dB, when supplied by the firmware.
    pub noise_db: Option<f32>,
}

/// Stationary-scene range FFT magnitudes emitted by the Out-of-Box demo.
///
/// Each entry is the received-antenna log2 magnitude for one range bin in
/// unsigned Q9 format. Converting a bin to metres requires the profile's range
/// resolution, which is configuration rather than wire data.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RangeProfile {
    pub bins_q9: Vec<u16>,
}

impl RangeProfile {
    /// Return a bin's log2 magnitude with its Q9 scaling removed.
    pub fn log2_magnitude(&self, index: usize) -> Option<f32> {
        self.bins_q9
            .get(index)
            .map(|value| f32::from(*value) / 512.0)
    }

    /// Index and raw Q9 value of the strongest range bin.
    pub fn peak_bin(&self) -> Option<(usize, u16)> {
        self.bins_q9
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, value)| *value)
    }
}

/// Per-frame timing and CPU-load measurements from TLV type 6.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessingStats {
    /// DSP processing time for the frame, in microseconds.
    pub inter_frame_processing_time_us: u32,
    /// UART transmission time for the previous frame, in microseconds.
    pub transmit_output_time_us: u32,
    /// Time left after processing the previous frame, in microseconds.
    pub inter_frame_processing_margin_us: u32,
    /// Inter-chirp margin in microseconds (not populated by xWR68xx OOB).
    pub inter_chirp_processing_margin_us: u32,
    /// CPU load during the active frame, in percent.
    pub active_frame_cpu_load_percent: u32,
    /// CPU load between frames, in percent.
    pub inter_frame_cpu_load_percent: u32,
}

/// Radar subsystem temperature report from TLV type 9.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemperatureStats {
    /// Raw TI report-valid word. Zero denotes a valid report.
    pub report_valid: u32,
    /// Radar-subsystem time since device power-up, in milliseconds.
    pub time_ms: u32,
    pub rx_c: [i16; 4],
    pub tx_c: [i16; 3],
    pub power_management_c: i16,
    pub digital_c: [i16; 2],
}

impl TemperatureStats {
    pub fn is_valid(&self) -> bool {
        self.report_valid == 0
    }

    /// Hottest digital-core sensor, suitable for a device-health display.
    pub fn processor_temperature_c(&self) -> Option<i16> {
        self.is_valid()
            .then(|| self.digital_c[0].max(self.digital_c[1]))
    }
}

/// One person held by the group tracker, from TLV 1010.
///
/// Axes match [`RadarPoint`]: `x` lateral, `y` outward from the antenna plane,
/// `z` vertical, all relative to the sensor and in metres. The tracker reports a
/// filtered state, not a measurement — a target coasts through frames where the
/// point cloud gave it nothing, which is exactly what makes
/// [`id`](Self::id) usable as a subject identity across frames.
#[cfg(feature = "vital-signs")]
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackedTarget {
    /// Track ID. The same person keeps it until the tracker drops the track,
    /// and it is what [`VitalSignsReading::subject_id`] refers to.
    pub id: u32,
    /// Position, in metres.
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Velocity, in metres per second.
    pub velocity_x: f32,
    pub velocity_y: f32,
    pub velocity_z: f32,
    /// Acceleration, in metres per second squared.
    pub acceleration_x: f32,
    pub acceleration_y: f32,
    pub acceleration_z: f32,
    /// The 4×4 error covariance matrix, row-major, as the tracker reports it.
    /// Kept raw: it is the filter's own uncertainty state, not a quantity this
    /// crate interprets.
    pub error_covariance: [f32; 16],
    /// Gating function gain.
    pub gating_gain: f32,
    /// The tracker's confidence in this track, `0.0..=1.0`.
    pub confidence: f32,
}

#[cfg(feature = "vital-signs")]
impl TrackedTarget {
    /// Ground-plane distance from the sensor, in metres.
    pub fn range_m(&self) -> f32 {
        self.x.hypot(self.y)
    }

    /// Speed irrespective of direction, in metres per second.
    pub fn speed_mps(&self) -> f32 {
        (self.velocity_x * self.velocity_x
            + self.velocity_y * self.velocity_y
            + self.velocity_z * self.velocity_z)
            .sqrt()
    }
}

/// What the tracker did with one point-cloud point, from TLV 1011.
///
/// One entry per point, in point order — but of the **previous** frame's cloud,
/// not this frame's. See
/// [`previous_point_associations`](RadarFrame::previous_point_associations).
///
/// The three rejection reasons are distinct values in TI's wire format and are
/// kept distinct here: "no target" and "outside the boundary box" are different
/// answers when you are deciding whether the sensor is aimed correctly.
#[cfg(feature = "vital-signs")]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PointAssociation {
    /// Assigned to the track with this ID.
    Target(u8),
    /// Not assigned: the point's SNR was too weak.
    WeakSnr,
    /// Not assigned: the point fell outside the configured boundary box.
    OutsideBoundary,
    /// Not assigned: the tracker judged the point noise.
    Noise,
}

#[cfg(feature = "vital-signs")]
impl PointAssociation {
    /// The track this point belongs to, if any.
    pub fn target_id(self) -> Option<u8> {
        match self {
            Self::Target(id) => Some(id),
            _ => None,
        }
    }
}

#[cfg(feature = "vital-signs")]
impl From<u8> for PointAssociation {
    fn from(value: u8) -> Self {
        match value {
            TRACK_INDEX_WEAK_SNR => Self::WeakSnr,
            TRACK_INDEX_OUTSIDE_BOUNDARY => Self::OutsideBoundary,
            TRACK_INDEX_NOISE => Self::Noise,
            id => Self::Target(id),
        }
    }
}

/// One raw result from TI's Vital Signs With People Tracking firmware.
///
/// The waveform arrays are unitless visualizer values. Use `heart_rate_bpm`
/// and `breathing_rate_bpm`; do not derive rates from those arrays.
#[cfg(feature = "vital-signs")]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VitalSignsReading {
    pub subject_id: u16,
    pub range_bin: u16,
    pub breathing_deviation: f32,
    pub heart_rate_bpm: f32,
    pub breathing_rate_bpm: f32,
    pub heart_waveform: [f32; 15],
    pub breath_waveform: [f32; 15],
}

/// One parsed UART frame.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct RadarFrame {
    pub protocol: RadarProtocol,
    pub header: FrameHeader,
    /// Cartesian dots ready to plot or aggregate.
    pub points: Vec<RadarPoint>,
    /// Stationary-scene range profile, when enabled by `guiMonitor`.
    #[serde(default)]
    pub range_profile: Option<RangeProfile>,
    /// Per-frame DSP timing and load, when enabled by `guiMonitor`.
    #[serde(default)]
    pub processing_stats: Option<ProcessingStats>,
    /// Radar subsystem temperatures, when supplied alongside stats.
    #[serde(default)]
    pub temperature_stats: Option<TemperatureStats>,
    /// People the group tracker is holding (TLV 1010).
    ///
    /// The tracker runs a frame behind the detection layer, so this is its state
    /// after consuming the *previous* frame's cloud. That costs one frame of
    /// latency — 90 ms on the shipped profile — and does not affect using
    /// [`TrackedTarget::id`] as a subject identity.
    #[cfg(feature = "vital-signs")]
    #[serde(default)]
    pub targets: Vec<TrackedTarget>,
    /// What the tracker did with each point of the **previous** frame's cloud,
    /// in that cloud's order (TLV 1011). Empty when the firmware did not send it.
    ///
    /// The lag is the same one that puts [`targets`](Self::targets) a frame
    /// behind, and it is measurable on the wire: across 13 consecutive frames
    /// from an IWR6843ISK this list's length matched the previous frame's point
    /// count every time and its own frame's not once. Pairing it with
    /// [`points`](Self::points) of the same frame therefore attributes each
    /// point to whatever the point in that slot happened to be last frame —
    /// plausible, ordered, and wrong. [`tracked_points`](Self::tracked_points)
    /// takes the previous frame explicitly so the pairing cannot be made by
    /// accident.
    #[cfg(feature = "vital-signs")]
    #[serde(default)]
    pub previous_point_associations: Vec<PointAssociation>,
    /// Raw vendor vital records, when the opt-in firmware protocol is enabled.
    #[cfg(feature = "vital-signs")]
    pub vital_signs: Vec<VitalSignsReading>,
    /// Types of well-formed TLVs that the selected parser skipped.
    pub unknown_tlv_types: Vec<u32>,
}

impl RadarFrame {
    pub fn frame_number(&self) -> u32 {
        self.header.frame_number
    }

    pub fn num_detected_points(&self) -> usize {
        self.points.len()
    }

    /// The tracked person a vital-signs record belongs to.
    ///
    /// `VitalSignsReading::subject_id` is a track ID, so a reading only says
    /// *where* the person is once TLV 1010 has been paired back to it.
    #[cfg(feature = "vital-signs")]
    pub fn target(&self, id: u32) -> Option<&TrackedTarget> {
        self.targets.iter().find(|target| target.id == id)
    }

    /// Points of `previous` that this frame's tracker output assigned to `id`.
    ///
    /// `previous` must be the frame immediately before this one — the
    /// association list arriving here describes *that* cloud, which is why the
    /// caller has to supply it rather than this method reaching for
    /// [`points`](Self::points). Yields nothing when TLV 1011 is absent, rather
    /// than pretending every point belongs to the target: the association is the
    /// firmware's judgement and this crate does not reconstruct it.
    #[cfg(feature = "vital-signs")]
    pub fn tracked_points<'a>(
        &'a self,
        previous: &'a RadarFrame,
        id: u8,
    ) -> impl Iterator<Item = &'a RadarPoint> {
        previous
            .points
            .iter()
            .zip(&self.previous_point_associations)
            .filter(move |(_, association)| **association == PointAssociation::Target(id))
            .map(|(point, _)| point)
    }
}

/// Why an IWR6843 UART packet could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseError {
    Truncated {
        context: &'static str,
        needed: usize,
        available: usize,
    },
    InvalidMagic([u8; 8]),
    InvalidPacketLength {
        declared: usize,
        actual: usize,
    },
    InvalidTlvLength {
        tlv_type: u32,
        declared: usize,
        expected: usize,
    },
    PointCountMismatch {
        declared: usize,
        parsed: usize,
    },
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated {
                context,
                needed,
                available,
            } => write!(
                f,
                "truncated {context}: need {needed} bytes, have {available}"
            ),
            Self::InvalidMagic(actual) => write!(f, "invalid radar magic word: {actual:02x?}"),
            Self::InvalidPacketLength { declared, actual } => write!(
                f,
                "packet length mismatch: header declares {declared} bytes, buffer has {actual}"
            ),
            Self::InvalidTlvLength {
                tlv_type,
                declared,
                expected,
            } => write!(
                f,
                "TLV type {tlv_type} declares {declared} payload bytes; expected {expected}"
            ),
            Self::PointCountMismatch { declared, parsed } => write!(
                f,
                "frame declares {declared} detected points but contains {parsed}"
            ),
        }
    }
}

impl Error for ParseError {}

/// Parse one complete Out-of-Box demo packet.
///
/// This compatible entry point always selects [`RadarProtocol::OutOfBox`].
pub fn parse_frame(frame: &[u8]) -> Result<RadarFrame, ParseError> {
    parse_frame_for(RadarProtocol::OutOfBox, frame)
}

/// Parse one complete packet using the explicitly selected firmware protocol.
pub fn parse_frame_for(protocol: RadarProtocol, frame: &[u8]) -> Result<RadarFrame, ParseError> {
    require(frame.len(), FRAME_HEADER_LEN, "frame header")?;

    let mut actual_magic = [0; 8];
    actual_magic.copy_from_slice(&frame[..8]);
    if actual_magic != MAGIC_WORD {
        return Err(ParseError::InvalidMagic(actual_magic));
    }

    let header = FrameHeader {
        version: u32_at(frame, 8),
        packet_length: u32_at(frame, 12),
        platform: u32_at(frame, 16),
        frame_number: u32_at(frame, 20),
        time_cpu_cycles: u32_at(frame, 24),
        num_detected_objects: u32_at(frame, 28),
        num_tlvs: u32_at(frame, 32),
        subframe_number: u32_at(frame, 36),
    };
    let packet_length = usize::try_from(header.packet_length).unwrap_or(usize::MAX);
    if packet_length != frame.len() || packet_length < FRAME_HEADER_LEN {
        return Err(ParseError::InvalidPacketLength {
            declared: packet_length,
            actual: frame.len(),
        });
    }

    let point_count = usize::try_from(header.num_detected_objects).unwrap_or(usize::MAX);
    let mut points = Vec::new();
    let mut side_info = Vec::new();
    let mut range_profile = None;
    let mut processing_stats = None;
    let mut temperature_stats = None;
    #[cfg(feature = "vital-signs")]
    let mut targets = Vec::new();
    #[cfg(feature = "vital-signs")]
    let mut previous_point_associations = Vec::new();
    #[cfg(feature = "vital-signs")]
    let mut vital_signs = Vec::new();
    let mut unknown_tlv_types = Vec::new();
    let mut offset = FRAME_HEADER_LEN;

    for _ in 0..header.num_tlvs {
        let available = packet_length.saturating_sub(offset);
        require(available, TLV_HEADER_LEN, "TLV header")?;

        let tlv_type = u32_at(frame, offset);
        let payload_len = usize::try_from(u32_at(frame, offset + 4)).unwrap_or(usize::MAX);
        let total_len = TLV_HEADER_LEN.saturating_add(payload_len);
        require(available, total_len, "TLV payload")?;
        let payload = &frame[offset + TLV_HEADER_LEN..offset + total_len];

        match protocol {
            RadarProtocol::OutOfBox => match tlv_type {
                DETECTED_POINTS => parse_cartesian_points(payload, point_count, &mut points)?,
                RANGE_PROFILE => range_profile = Some(parse_range_profile(payload)?),
                PROCESSING_STATS => processing_stats = Some(parse_processing_stats(payload)?),
                DETECTED_POINTS_SIDE_INFO => {
                    parse_side_info(payload, point_count, &mut side_info)?;
                }
                TEMPERATURE_STATS => {
                    temperature_stats = Some(parse_temperature_stats(payload)?);
                }
                unknown => unknown_tlv_types.push(unknown),
            },
            #[cfg(feature = "vital-signs")]
            RadarProtocol::VitalSigns => match tlv_type {
                COMPRESSED_SPHERICAL_POINTS => {
                    parse_compressed_points(payload, point_count, &mut points)?;
                }
                TRACKER_TARGET_LIST => parse_tracked_targets(payload, &mut targets)?,
                TRACKER_TARGET_INDEX => {
                    previous_point_associations
                        .extend(payload.iter().copied().map(PointAssociation::from));
                }
                VITAL_SIGNS => vital_signs.push(parse_vital_signs(payload)?),
                unknown => unknown_tlv_types.push(unknown),
            },
        }
        offset += total_len;
    }

    // Bytes between the last TLV and `packet_length` are deliberately not
    // examined. The demo rounds `totalPacketLen` up to a multiple of
    // `MMWDEMO_OUTPUT_MSG_SEGMENT_LEN` (32) and then transmits the slack from an
    // uninitialized stack array — `uint8_t padding[MMWDEMO_OUTPUT_MSG_SEGMENT_LEN]`
    // in `MmwDemo_transmitProcessedOutput`, declared and written to the UART
    // without ever being assigned. TI specifies the packet *length* only ("output
    // packet length is a multiple of this value"); the padding *content* is
    // unspecified, and in practice is whatever was on that stack. Requiring zeros
    // rejected 71% of frames from a live IWR6843.
    if points.len() != point_count {
        return Err(ParseError::PointCountMismatch {
            declared: point_count,
            parsed: points.len(),
        });
    }
    if !side_info.is_empty() {
        for (point, (snr_db, noise_db)) in points.iter_mut().zip(side_info) {
            point.snr_db = Some(snr_db);
            point.noise_db = Some(noise_db);
        }
    }

    Ok(RadarFrame {
        protocol,
        header,
        points,
        range_profile,
        processing_stats,
        temperature_stats,
        #[cfg(feature = "vital-signs")]
        targets,
        #[cfg(feature = "vital-signs")]
        previous_point_associations,
        #[cfg(feature = "vital-signs")]
        vital_signs,
        unknown_tlv_types,
    })
}

/// Decode TLV 1010 — zero or more 112-byte tracker states.
#[cfg(feature = "vital-signs")]
fn parse_tracked_targets(
    payload: &[u8],
    targets: &mut Vec<TrackedTarget>,
) -> Result<(), ParseError> {
    // A trailing partial record would mean the stride is wrong, and reading a
    // target out of a mis-strided buffer produces coordinates that look real.
    if !payload.len().is_multiple_of(TRACKER_TARGET_LEN) {
        return Err(ParseError::InvalidTlvLength {
            tlv_type: TRACKER_TARGET_LIST,
            declared: payload.len(),
            expected: payload.len().next_multiple_of(TRACKER_TARGET_LEN),
        });
    }

    targets.reserve(payload.len() / TRACKER_TARGET_LEN);
    for raw in payload.chunks_exact(TRACKER_TARGET_LEN) {
        let mut error_covariance = [0.0; 16];
        for (index, cell) in error_covariance.iter_mut().enumerate() {
            *cell = f32_at(raw, 40 + index * 4);
        }
        targets.push(TrackedTarget {
            id: u32_at(raw, 0),
            x: f32_at(raw, 4),
            y: f32_at(raw, 8),
            z: f32_at(raw, 12),
            velocity_x: f32_at(raw, 16),
            velocity_y: f32_at(raw, 20),
            velocity_z: f32_at(raw, 24),
            acceleration_x: f32_at(raw, 28),
            acceleration_y: f32_at(raw, 32),
            acceleration_z: f32_at(raw, 36),
            error_covariance,
            gating_gain: f32_at(raw, 104),
            confidence: f32_at(raw, 108),
        });
    }
    Ok(())
}

fn parse_range_profile(payload: &[u8]) -> Result<RangeProfile, ParseError> {
    if !payload.len().is_multiple_of(RANGE_PROFILE_BIN_LEN) {
        return Err(ParseError::InvalidTlvLength {
            tlv_type: RANGE_PROFILE,
            declared: payload.len(),
            expected: payload.len().saturating_add(1),
        });
    }

    Ok(RangeProfile {
        bins_q9: payload
            .chunks_exact(RANGE_PROFILE_BIN_LEN)
            .map(|bin| u16::from_le_bytes([bin[0], bin[1]]))
            .collect(),
    })
}

fn parse_processing_stats(payload: &[u8]) -> Result<ProcessingStats, ParseError> {
    validate_tlv_length(PROCESSING_STATS, payload.len(), PROCESSING_STATS_LEN)?;
    Ok(ProcessingStats {
        inter_frame_processing_time_us: u32_at(payload, 0),
        transmit_output_time_us: u32_at(payload, 4),
        inter_frame_processing_margin_us: u32_at(payload, 8),
        inter_chirp_processing_margin_us: u32_at(payload, 12),
        active_frame_cpu_load_percent: u32_at(payload, 16),
        inter_frame_cpu_load_percent: u32_at(payload, 20),
    })
}

fn parse_temperature_stats(payload: &[u8]) -> Result<TemperatureStats, ParseError> {
    validate_tlv_length(TEMPERATURE_STATS, payload.len(), TEMPERATURE_STATS_LEN)?;
    Ok(TemperatureStats {
        report_valid: u32_at(payload, 0),
        time_ms: u32_at(payload, 4),
        rx_c: [
            i16_at(payload, 8),
            i16_at(payload, 10),
            i16_at(payload, 12),
            i16_at(payload, 14),
        ],
        tx_c: [
            i16_at(payload, 16),
            i16_at(payload, 18),
            i16_at(payload, 20),
        ],
        power_management_c: i16_at(payload, 22),
        digital_c: [i16_at(payload, 24), i16_at(payload, 26)],
    })
}

fn parse_cartesian_points(
    payload: &[u8],
    point_count: usize,
    points: &mut Vec<RadarPoint>,
) -> Result<(), ParseError> {
    validate_tlv_length(
        DETECTED_POINTS,
        payload.len(),
        point_count.saturating_mul(POINT_LEN),
    )?;
    points.reserve(point_count);
    for point in payload.chunks_exact(POINT_LEN) {
        points.push(RadarPoint {
            x: f32_at(point, 0),
            y: f32_at(point, 4),
            z: f32_at(point, 8),
            velocity: f32_at(point, 12),
            snr_db: None,
            noise_db: None,
        });
    }
    Ok(())
}

fn parse_side_info(
    payload: &[u8],
    point_count: usize,
    side_info: &mut Vec<(f32, f32)>,
) -> Result<(), ParseError> {
    validate_tlv_length(
        DETECTED_POINTS_SIDE_INFO,
        payload.len(),
        point_count.saturating_mul(SIDE_INFO_LEN),
    )?;
    side_info.reserve(point_count);
    for info in payload.chunks_exact(SIDE_INFO_LEN) {
        side_info.push((
            f32::from(i16_at(info, 0)) * 0.1,
            f32::from(i16_at(info, 2)) * 0.1,
        ));
    }
    Ok(())
}

#[cfg(feature = "vital-signs")]
fn parse_compressed_points(
    payload: &[u8],
    point_count: usize,
    points: &mut Vec<RadarPoint>,
) -> Result<(), ParseError> {
    let expected =
        COMPRESSED_POINT_UNITS_LEN.saturating_add(point_count.saturating_mul(COMPRESSED_POINT_LEN));
    validate_tlv_length(COMPRESSED_SPHERICAL_POINTS, payload.len(), expected)?;

    let elevation_unit = f32_at(payload, 0);
    let azimuth_unit = f32_at(payload, 4);
    let doppler_unit = f32_at(payload, 8);
    let range_unit = f32_at(payload, 12);
    let snr_unit = f32_at(payload, 16);

    points.reserve(point_count);
    for raw in payload[COMPRESSED_POINT_UNITS_LEN..].chunks_exact(COMPRESSED_POINT_LEN) {
        let elevation = f32::from(raw[0] as i8) * elevation_unit;
        let azimuth = f32::from(raw[1] as i8) * azimuth_unit;
        let velocity = f32::from(i16_at(raw, 2)) * doppler_unit;
        let range = f32::from(u16_at(raw, 4)) * range_unit;
        let snr_db = f32::from(u16_at(raw, 6)) * snr_unit;
        let horizontal_range = range * elevation.cos();

        points.push(RadarPoint {
            x: horizontal_range * azimuth.sin(),
            y: horizontal_range * azimuth.cos(),
            z: range * elevation.sin(),
            velocity,
            snr_db: Some(snr_db),
            noise_db: None,
        });
    }
    Ok(())
}

#[cfg(feature = "vital-signs")]
fn parse_vital_signs(payload: &[u8]) -> Result<VitalSignsReading, ParseError> {
    validate_tlv_length(VITAL_SIGNS, payload.len(), VITAL_SIGNS_LEN)?;
    let mut heart_waveform = [0.0; 15];
    let mut breath_waveform = [0.0; 15];
    for (index, sample) in heart_waveform.iter_mut().enumerate() {
        *sample = f32_at(payload, 16 + index * 4);
    }
    for (index, sample) in breath_waveform.iter_mut().enumerate() {
        *sample = f32_at(payload, 76 + index * 4);
    }

    Ok(VitalSignsReading {
        subject_id: u16_at(payload, 0),
        range_bin: u16_at(payload, 2),
        breathing_deviation: f32_at(payload, 4),
        heart_rate_bpm: f32_at(payload, 8),
        breathing_rate_bpm: f32_at(payload, 12),
        heart_waveform,
        breath_waveform,
    })
}

fn require(available: usize, needed: usize, context: &'static str) -> Result<(), ParseError> {
    if available < needed {
        return Err(ParseError::Truncated {
            context,
            needed,
            available,
        });
    }
    Ok(())
}

fn validate_tlv_length(tlv_type: u32, declared: usize, expected: usize) -> Result<(), ParseError> {
    if declared != expected {
        return Err(ParseError::InvalidTlvLength {
            tlv_type,
            declared,
            expected,
        });
    }
    Ok(())
}

fn i16_at(bytes: &[u8], offset: usize) -> i16 {
    i16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(feature = "vital-signs")]
fn u16_at(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

pub(crate) fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_bits(u32_at(bytes, offset))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_plot_points_and_side_info() {
        let points = point_payload(&[[1.25, -0.5, 2.75, -1.0], [-2.0, 3.0, 0.25, 0.5]]);
        let side_info = side_info_payload(&[(123, 45), (300, 100)]);
        let frame = make_frame(
            17,
            2,
            &[
                (DETECTED_POINTS, &points),
                (DETECTED_POINTS_SIDE_INFO, &side_info),
            ],
        );

        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.frame_number(), 17);
        assert_eq!(parsed.num_detected_points(), 2);
        assert_eq!(
            parsed.points[0],
            RadarPoint {
                x: 1.25,
                y: -0.5,
                z: 2.75,
                velocity: -1.0,
                snr_db: Some(12.3),
                noise_db: Some(4.5),
            }
        );
        assert_eq!(parsed.points[1].snr_db, Some(30.0));
    }

    #[test]
    fn parses_points_without_optional_side_info() {
        let points = point_payload(&[[1.0, 2.0, 3.0, 4.0]]);
        let frame = make_frame(1, 1, &[(DETECTED_POINTS, &points)]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.points[0].snr_db, None);
    }

    #[test]
    fn accepts_zero_point_frame_without_point_tlv() {
        let frame = make_frame(1, 0, &[]);
        assert!(parse_frame(&frame).unwrap().points.is_empty());
    }

    #[test]
    fn skips_other_visualizer_tlvs_and_padding() {
        let points = point_payload(&[[1.0, 2.0, 3.0, 4.0]]);
        let frame = make_frame(1, 1, &[(5, &[1, 2]), (DETECTED_POINTS, &points)]);
        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.unknown_tlv_types, [5]);
    }

    /// The demo transmits its 32-byte alignment slack from an uninitialized
    /// stack array, so the padding is whatever that stack held — TI specifies
    /// the packet length, never the padding content. These are the bytes a live
    /// IWR6843 actually sent; requiring zeros here rejected 71% of its frames.
    #[test]
    fn accepts_the_uninitialized_padding_the_demo_transmits() {
        let points = point_payload(&[[1.0, 2.0, 3.0, 4.0]]);
        let mut frame = make_frame(1, 1, &[(DETECTED_POINTS, &points)]);
        let padding_start = frame.len() - 12;
        frame[padding_start..].copy_from_slice(&[
            0x44, 0xd6, 0x00, 0x08, 0x01, 0x00, 0x00, 0x00, 0x38, 0x3b, 0x00, 0x08,
        ]);

        let parsed = parse_frame(&frame).unwrap();
        assert_eq!(parsed.points.len(), 1);
    }

    #[test]
    fn parses_range_profile_processing_and_temperature_stats() {
        let range = u16_payload(&[0, 512, 1_024, u16::MAX]);
        let stats = u32_payload(&[1_500, 250, 8_000, 0, 37, 12]);
        let temperatures =
            temperature_payload(0, 98_765, [-5, 20, 21, 22], [30, 31, 32], 33, [44, 46]);
        let frame = make_frame(
            91,
            0,
            &[
                (RANGE_PROFILE, &range),
                (PROCESSING_STATS, &stats),
                (TEMPERATURE_STATS, &temperatures),
            ],
        );

        let parsed = parse_frame(&frame).unwrap();
        let profile = parsed.range_profile.unwrap();
        assert_eq!(profile.bins_q9, [0, 512, 1_024, u16::MAX]);
        assert_eq!(profile.log2_magnitude(1), Some(1.0));
        assert_eq!(profile.peak_bin(), Some((3, u16::MAX)));

        let stats = parsed.processing_stats.unwrap();
        assert_eq!(stats.inter_frame_processing_time_us, 1_500);
        assert_eq!(stats.transmit_output_time_us, 250);
        assert_eq!(stats.inter_frame_processing_margin_us, 8_000);
        assert_eq!(stats.inter_chirp_processing_margin_us, 0);
        assert_eq!(stats.active_frame_cpu_load_percent, 37);
        assert_eq!(stats.inter_frame_cpu_load_percent, 12);

        let temperatures = parsed.temperature_stats.unwrap();
        assert!(temperatures.is_valid());
        assert_eq!(temperatures.time_ms, 98_765);
        assert_eq!(temperatures.rx_c, [-5, 20, 21, 22]);
        assert_eq!(temperatures.tx_c, [30, 31, 32]);
        assert_eq!(temperatures.power_management_c, 33);
        assert_eq!(temperatures.digital_c, [44, 46]);
        assert_eq!(temperatures.processor_temperature_c(), Some(46));
        assert!(parsed.unknown_tlv_types.is_empty());
    }

    #[test]
    fn rejects_malformed_monitor_tlv_lengths() {
        for (tlv_type, payload) in [
            (RANGE_PROFILE, vec![0; 3]),
            (PROCESSING_STATS, vec![0; PROCESSING_STATS_LEN - 1]),
            (TEMPERATURE_STATS, vec![0; TEMPERATURE_STATS_LEN + 1]),
        ] {
            let frame = make_frame(1, 0, &[(tlv_type, &payload)]);
            assert!(matches!(
                parse_frame(&frame),
                Err(ParseError::InvalidTlvLength {
                    tlv_type: actual,
                    ..
                }) if actual == tlv_type
            ));
        }
    }

    #[test]
    fn preserves_invalid_temperature_report_without_exposing_a_reading() {
        let temperatures = temperature_payload(1, 20, [1; 4], [2; 3], 3, [4, 5]);
        let frame = make_frame(1, 0, &[(TEMPERATURE_STATS, &temperatures)]);
        let parsed = parse_frame(&frame).unwrap();
        let temperatures = parsed.temperature_stats.unwrap();
        assert!(!temperatures.is_valid());
        assert_eq!(temperatures.processor_temperature_c(), None);
    }

    #[test]
    fn rejects_wrong_point_count() {
        let points = point_payload(&[[1.0, 2.0, 3.0, 4.0]]);
        let frame = make_frame(1, 2, &[(DETECTED_POINTS, &points)]);
        assert!(matches!(
            parse_frame(&frame),
            Err(ParseError::InvalidTlvLength {
                tlv_type: DETECTED_POINTS,
                ..
            })
        ));
    }

    #[test]
    fn rejects_truncated_tlv() {
        let points = point_payload(&[[1.0, 2.0, 3.0, 4.0]]);
        let mut frame = make_frame(1, 1, &[(DETECTED_POINTS, &points)]);
        frame.truncate(frame.len() - 4);
        let new_length = frame.len() as u32;
        frame[12..16].copy_from_slice(&new_length.to_le_bytes());
        assert!(matches!(
            parse_frame(&frame),
            Err(ParseError::Truncated {
                context: "TLV payload",
                ..
            })
        ));
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn parses_compressed_points_and_vital_records() {
        let points =
            compressed_point_payload([0.01, 0.01, 0.1, 0.01, 0.5], &[(10, 20, -3, 200, 40)]);
        let vital_a = vital_payload(7, 44, 0.25, 72.0, 15.0);
        let vital_b = vital_payload(9, 55, 0.5, 81.0, 18.0);
        let frame = make_frame(
            5,
            1,
            &[
                (COMPRESSED_SPHERICAL_POINTS, &points),
                (VITAL_SIGNS, &vital_a),
                (VITAL_SIGNS, &vital_b),
            ],
        );

        let parsed = parse_frame_for(RadarProtocol::VitalSigns, &frame).unwrap();
        let expected_range = 2.0;
        assert!((parsed.points[0].x - expected_range * 0.1_f32.cos() * 0.2_f32.sin()).abs() < 1e-5);
        assert!((parsed.points[0].y - expected_range * 0.1_f32.cos() * 0.2_f32.cos()).abs() < 1e-5);
        assert!((parsed.points[0].z - expected_range * 0.1_f32.sin()).abs() < 1e-5);
        assert_eq!(parsed.points[0].velocity, -0.3);
        assert_eq!(parsed.points[0].snr_db, Some(20.0));
        assert_eq!(parsed.vital_signs.len(), 2);
        assert_eq!(parsed.vital_signs[0].subject_id, 7);
        assert_eq!(parsed.vital_signs[0].range_bin, 44);
        assert_eq!(parsed.vital_signs[0].heart_rate_bpm, 72.0);
        assert_eq!(parsed.vital_signs[0].breathing_rate_bpm, 15.0);
        assert_eq!(parsed.vital_signs[0].heart_waveform[14], 14.0);
        assert_eq!(parsed.vital_signs[0].breath_waveform[14], 114.0);
    }

    /// Every field of the 112-byte tracker state, at its own offset, with signed
    /// and fractional values that a wrong stride or a swapped pair would move.
    #[cfg(feature = "vital-signs")]
    #[test]
    fn parses_the_tracker_target_list() {
        let first = target_payload(
            3,
            [-1.25, 2.5, 0.75],
            [0.5, -0.25, 0.125],
            [-0.5, 0.25, 0.0],
        );
        let second = target_payload(9, [0.0, 4.0, 1.5], [0.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
        let mut payload = first;
        payload.extend_from_slice(&second);
        let frame = make_frame(11, 0, &[(TRACKER_TARGET_LIST, &payload)]);

        let parsed = parse_frame_for(RadarProtocol::VitalSigns, &frame).unwrap();
        assert_eq!(parsed.targets.len(), 2);

        let target = &parsed.targets[0];
        assert_eq!(target.id, 3);
        assert_eq!((target.x, target.y, target.z), (-1.25, 2.5, 0.75));
        assert_eq!(
            (target.velocity_x, target.velocity_y, target.velocity_z),
            (0.5, -0.25, 0.125)
        );
        assert_eq!(
            (
                target.acceleration_x,
                target.acceleration_y,
                target.acceleration_z
            ),
            (-0.5, 0.25, 0.0)
        );
        // The covariance is written as 0.0..16.0, so a shifted read lands on the
        // wrong index rather than on a plausible-looking zero.
        assert_eq!(target.error_covariance[0], 0.0);
        assert_eq!(target.error_covariance[15], 15.0);
        assert_eq!(target.gating_gain, 42.0);
        assert_eq!(target.confidence, 0.875);

        assert_eq!(parsed.targets[1].id, 9);
        assert_eq!(parsed.target(9).unwrap().y, 4.0);
        assert!(parsed.target(4).is_none());
        assert!(parsed.unknown_tlv_types.is_empty());

        assert!((parsed.targets[0].range_m() - 1.25_f32.hypot(2.5)).abs() < 1e-6);
        assert!((parsed.targets[1].speed_mps()).abs() < 1e-6);
    }

    /// A stride that does not divide the payload means the record layout is not
    /// what this build believes, and every field read after it is fiction.
    #[cfg(feature = "vital-signs")]
    #[test]
    fn rejects_a_partial_tracker_target() {
        let frame = make_frame(1, 0, &[(TRACKER_TARGET_LIST, &[0; TRACKER_TARGET_LEN + 8])]);
        assert!(matches!(
            parse_frame_for(RadarProtocol::VitalSigns, &frame),
            Err(ParseError::InvalidTlvLength {
                tlv_type: TRACKER_TARGET_LIST,
                declared: 120,
                expected: 224,
            })
        ));
    }

    /// TLV 1011 is one byte per point, in point order, with three sentinels that
    /// say why a point was rejected rather than which track took it — and it
    /// describes the PREVIOUS frame's cloud, which is what
    /// [`RadarFrame::tracked_points`] pairs it against.
    #[cfg(feature = "vital-signs")]
    #[test]
    fn pairs_associations_with_the_previous_frames_points() {
        // Frame 12 carries five points; frame 13 carries a different number, so
        // a same-frame zip could not even produce these results by accident.
        let earlier_points = compressed_point_payload(
            [0.01, 0.01, 0.1, 0.01, 0.5],
            &[
                (0, 0, 0, 100, 20),
                (0, 0, 0, 110, 20),
                (0, 0, 0, 120, 20),
                (0, 0, 0, 130, 20),
                (0, 0, 0, 140, 40),
            ],
        );
        let previous = parse_frame_for(
            RadarProtocol::VitalSigns,
            &make_frame(12, 5, &[(COMPRESSED_SPHERICAL_POINTS, &earlier_points)]),
        )
        .unwrap();

        let later_points =
            compressed_point_payload([0.01, 0.01, 0.1, 0.01, 0.5], &[(0, 0, 0, 200, 60)]);
        let index = [
            0,
            7,
            TRACK_INDEX_WEAK_SNR,
            TRACK_INDEX_OUTSIDE_BOUNDARY,
            TRACK_INDEX_NOISE,
        ];
        let current = parse_frame_for(
            RadarProtocol::VitalSigns,
            &make_frame(
                13,
                1,
                &[
                    (COMPRESSED_SPHERICAL_POINTS, &later_points),
                    (TRACKER_TARGET_INDEX, &index),
                ],
            ),
        )
        .unwrap();

        assert_eq!(
            current.previous_point_associations,
            [
                PointAssociation::Target(0),
                PointAssociation::Target(7),
                PointAssociation::WeakSnr,
                PointAssociation::OutsideBoundary,
                PointAssociation::Noise,
            ]
        );
        // The list is as long as the PREVIOUS cloud, not this frame's.
        assert_eq!(
            current.previous_point_associations.len(),
            previous.points.len()
        );
        assert_eq!(current.points.len(), 1);
        assert_eq!(current.previous_point_associations[0].target_id(), Some(0));
        assert_eq!(current.previous_point_associations[2].target_id(), None);

        // Track 7 took the previous cloud's second point — the 40-SNR one is the
        // fifth, so picking it here would mean the zip ran off the wrong list.
        let assigned: Vec<_> = current.tracked_points(&previous, 7).collect();
        assert_eq!(assigned.len(), 1);
        assert_eq!(assigned[0].snr_db, Some(10.0));
        assert_eq!(current.tracked_points(&previous, 200).count(), 0);
        assert!(current.unknown_tlv_types.is_empty());
    }

    /// Without TLV 1011 nothing is assumed: no association is not "all mine".
    #[cfg(feature = "vital-signs")]
    #[test]
    fn without_an_index_no_point_belongs_to_any_target() {
        let points = compressed_point_payload([0.01, 0.01, 0.1, 0.01, 0.5], &[(0, 0, 0, 100, 20)]);
        let frame = make_frame(1, 1, &[(COMPRESSED_SPHERICAL_POINTS, &points)]);

        let parsed = parse_frame_for(RadarProtocol::VitalSigns, &frame).unwrap();
        assert_eq!(parsed.points.len(), 1);
        assert!(parsed.previous_point_associations.is_empty());
        assert_eq!(parsed.tracked_points(&parsed, 0).count(), 0);
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn rejects_malformed_vital_record() {
        let frame = make_frame(1, 0, &[(VITAL_SIGNS, &[0; VITAL_SIGNS_LEN - 1])]);
        assert!(matches!(
            parse_frame_for(RadarProtocol::VitalSigns, &frame),
            Err(ParseError::InvalidTlvLength {
                tlv_type: VITAL_SIGNS,
                ..
            })
        ));
    }

    #[cfg(feature = "vital-signs")]
    #[test]
    fn explicit_protocol_rejects_a_vital_point_frame_as_out_of_box() {
        let points = compressed_point_payload([0.01, 0.01, 0.1, 0.01, 0.5], &[(0, 0, 0, 100, 20)]);
        let frame = make_frame(1, 1, &[(COMPRESSED_SPHERICAL_POINTS, &points)]);
        assert!(matches!(
            parse_frame(&frame),
            Err(ParseError::PointCountMismatch {
                declared: 1,
                parsed: 0
            })
        ));
    }

    fn point_payload(points: &[[f32; 4]]) -> Vec<u8> {
        points
            .iter()
            .flat_map(|point| point.iter())
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn side_info_payload(values: &[(i16, i16)]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|(snr, noise)| [snr.to_le_bytes(), noise.to_le_bytes()])
            .flatten()
            .collect()
    }

    fn u16_payload(values: &[u16]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn u32_payload(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect()
    }

    fn temperature_payload(
        valid: u32,
        time_ms: u32,
        rx_c: [i16; 4],
        tx_c: [i16; 3],
        power_management_c: i16,
        digital_c: [i16; 2],
    ) -> Vec<u8> {
        let mut payload = Vec::with_capacity(TEMPERATURE_STATS_LEN);
        payload.extend_from_slice(&valid.to_le_bytes());
        payload.extend_from_slice(&time_ms.to_le_bytes());
        for temperature in rx_c
            .into_iter()
            .chain(tx_c)
            .chain([power_management_c])
            .chain(digital_c)
        {
            payload.extend_from_slice(&temperature.to_le_bytes());
        }
        payload
    }

    #[cfg(feature = "vital-signs")]
    fn compressed_point_payload(units: [f32; 5], points: &[(i8, i8, i16, u16, u16)]) -> Vec<u8> {
        let mut payload = units
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect::<Vec<_>>();
        for (elevation, azimuth, doppler, range, snr) in points {
            payload.push(*elevation as u8);
            payload.push(*azimuth as u8);
            payload.extend_from_slice(&doppler.to_le_bytes());
            payload.extend_from_slice(&range.to_le_bytes());
            payload.extend_from_slice(&snr.to_le_bytes());
        }
        payload
    }

    /// One 112-byte tracker record. The covariance is filled with `0.0..16.0` so
    /// a misaligned read is visible rather than plausible.
    #[cfg(feature = "vital-signs")]
    fn target_payload(
        id: u32,
        position: [f32; 3],
        velocity: [f32; 3],
        acceleration: [f32; 3],
    ) -> Vec<u8> {
        let mut payload = Vec::with_capacity(TRACKER_TARGET_LEN);
        payload.extend_from_slice(&id.to_le_bytes());
        for value in position.into_iter().chain(velocity).chain(acceleration) {
            payload.extend_from_slice(&value.to_le_bytes());
        }
        for cell in 0..16 {
            payload.extend_from_slice(&(cell as f32).to_le_bytes());
        }
        payload.extend_from_slice(&42.0_f32.to_le_bytes());
        payload.extend_from_slice(&0.875_f32.to_le_bytes());
        debug_assert_eq!(payload.len(), TRACKER_TARGET_LEN);
        payload
    }

    #[cfg(feature = "vital-signs")]
    fn vital_payload(
        subject_id: u16,
        range_bin: u16,
        deviation: f32,
        heart_rate: f32,
        breathing_rate: f32,
    ) -> Vec<u8> {
        let mut payload = Vec::with_capacity(VITAL_SIGNS_LEN);
        payload.extend_from_slice(&subject_id.to_le_bytes());
        payload.extend_from_slice(&range_bin.to_le_bytes());
        payload.extend_from_slice(&deviation.to_le_bytes());
        payload.extend_from_slice(&heart_rate.to_le_bytes());
        payload.extend_from_slice(&breathing_rate.to_le_bytes());
        for sample in 0..15 {
            payload.extend_from_slice(&(sample as f32).to_le_bytes());
        }
        for sample in 100..115 {
            payload.extend_from_slice(&(sample as f32).to_le_bytes());
        }
        payload
    }

    pub(crate) fn make_frame(
        frame_number: u32,
        point_count: u32,
        tlvs: &[(u32, &[u8])],
    ) -> Vec<u8> {
        let unpadded_len = FRAME_HEADER_LEN
            + tlvs
                .iter()
                .map(|(_, payload)| TLV_HEADER_LEN + payload.len())
                .sum::<usize>();
        let packet_length = unpadded_len.next_multiple_of(32);
        let mut frame = vec![0; FRAME_HEADER_LEN];
        frame[..8].copy_from_slice(&MAGIC_WORD);
        write_u32(&mut frame, 8, 0x03_06_00_00);
        write_u32(&mut frame, 12, packet_length as u32);
        write_u32(&mut frame, 16, 0x000a_6843);
        write_u32(&mut frame, 20, frame_number);
        write_u32(&mut frame, 24, 123_456);
        write_u32(&mut frame, 28, point_count);
        write_u32(&mut frame, 32, tlvs.len() as u32);

        for (tlv_type, payload) in tlvs {
            frame.extend_from_slice(&tlv_type.to_le_bytes());
            frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
            frame.extend_from_slice(payload);
        }
        frame.resize(packet_length, 0);
        frame
    }

    fn write_u32(bytes: &mut [u8], offset: usize, value: u32) {
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
}
