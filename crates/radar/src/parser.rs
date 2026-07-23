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

const DETECTED_POINTS: u32 = 1;
const DETECTED_POINTS_SIDE_INFO: u32 = 7;
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

/// UART payload protocol selected by the firmware flashed on the radar.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RadarProtocol {
    /// Factory/out-of-box mmWave demo with Cartesian point TLVs.
    #[default]
    OutOfBox,
    /// Radar Toolbox Vital Signs With People Tracking demo.
    #[cfg(feature = "vital-signs")]
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
    NonZeroPadding,
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
            Self::NonZeroPadding => f.write_str("non-zero data remains after the declared TLVs"),
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
                DETECTED_POINTS_SIDE_INFO => {
                    parse_side_info(payload, point_count, &mut side_info)?;
                }
                unknown => unknown_tlv_types.push(unknown),
            },
            #[cfg(feature = "vital-signs")]
            RadarProtocol::VitalSigns => match tlv_type {
                COMPRESSED_SPHERICAL_POINTS => {
                    parse_compressed_points(payload, point_count, &mut points)?;
                }
                VITAL_SIGNS => vital_signs.push(parse_vital_signs(payload)?),
                unknown => unknown_tlv_types.push(unknown),
            },
        }
        offset += total_len;
    }

    if frame[offset..packet_length].iter().any(|byte| *byte != 0) {
        return Err(ParseError::NonZeroPadding);
    }
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
        #[cfg(feature = "vital-signs")]
        vital_signs,
        unknown_tlv_types,
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
