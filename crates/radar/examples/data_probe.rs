// SPDX-License-Identifier: Apache-2.0

//! Read the data UART and report what the crate makes of it.
//!
//! The other half of [`cli_probe`](../cli_probe.rs): once the sensor has been
//! configured and is streaming, this says whether the bytes actually parse — how
//! many points each frame carries, how many TLVs this build does not recognize,
//! and what the indicators make of it. Parse errors are printed and counted
//! rather than fatal, since which frames fail is usually the diagnosis.
//!
//! ```text
//! data_probe /dev/ttyUSB1 20
//! ```

use std::{env, process::ExitCode, time::Instant};

use snf_radar::{IndicatorEngine, RadarConfig, RadarProtocol, RadarStream};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let port = arguments
        .next()
        .unwrap_or_else(|| "/dev/ttyUSB1".to_string());
    let wanted: usize = arguments
        .next()
        .and_then(|count| count.parse().ok())
        .unwrap_or(10);
    // The protocol is never guessed: both demos share the magic word and the
    // 40-byte header, so the wrong one reads plausible nonsense.
    let protocol = match arguments.next().as_deref() {
        None => RadarProtocol::default(),
        Some("out-of-box") => RadarProtocol::OutOfBox,
        #[cfg(feature = "vital-signs")]
        Some("vital-signs") => RadarProtocol::VitalSigns,
        Some(other) => {
            eprintln!("data_probe: unknown protocol {other:?}");
            return ExitCode::FAILURE;
        }
    };

    let mut radar = match RadarStream::open(RadarConfig {
        data_port: port.clone(),
        protocol,
        ..RadarConfig::default()
    }) {
        Ok(radar) => radar,
        Err(error) => {
            eprintln!("data_probe: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!("data_probe: {port} @ 921600, reading {wanted} frames");

    let mut indicators = IndicatorEngine::default();
    let mut seen = 0;
    let mut errors = 0;
    // Kept because the tracker's association list refers to it, not to the frame
    // it arrives in.
    #[cfg(feature = "vital-signs")]
    let mut previous: Option<snf_radar::RadarFrame> = None;
    while seen < wanted {
        match radar.next_frame().await {
            Ok(Some(frame)) => {
                seen += 1;
                let snapshot = indicators.update(Instant::now(), &frame);
                println!(
                    "  frame {} — {} points, {} unknown tlv(s), rms {:.4} m/s, moving {:.2}, confidence {:.2}",
                    frame.frame_number(),
                    frame.num_detected_points(),
                    frame.unknown_tlv_types.len(),
                    snapshot.activity.rms_radial_speed_mps,
                    snapshot.activity.moving_point_fraction,
                    snapshot.activity.confidence,
                );
                if let Some(profile) = &frame.range_profile {
                    if let Some((bin, q9)) = profile.peak_bin() {
                        println!(
                            "    range — {} bins, peak bin {bin} (Q9 {q9}, log2 {:.3})",
                            profile.bins_q9.len(),
                            f32::from(q9) / 512.0,
                        );
                    } else {
                        println!("    range — empty profile");
                    }
                }
                if let Some(stats) = &frame.processing_stats {
                    println!(
                        "    DSP — frame {} us, UART {} us, margin {} us, CPU active {}%, idle {}%",
                        stats.inter_frame_processing_time_us,
                        stats.transmit_output_time_us,
                        stats.inter_frame_processing_margin_us,
                        stats.active_frame_cpu_load_percent,
                        stats.inter_frame_cpu_load_percent,
                    );
                }
                if let Some(temperatures) = &frame.temperature_stats {
                    if temperatures.is_valid() {
                        println!(
                            "    temperature — RX {:?}°C, TX {:?}°C, PM {}°C, digital {:?}°C @ {} ms",
                            temperatures.rx_c,
                            temperatures.tx_c,
                            temperatures.power_management_c,
                            temperatures.digital_c,
                            temperatures.time_ms,
                        );
                    } else {
                        println!(
                            "    temperature — report unavailable (rlRfGetTemperatureReport error {})",
                            temperatures.report_valid,
                        );
                    }
                }
                #[cfg(feature = "vital-signs")]
                for target in &frame.targets {
                    // The association list describes the previous frame's cloud,
                    // so the point count needs that frame in hand.
                    let assigned = previous
                        .as_ref()
                        .map(|earlier| frame.tracked_points(earlier, target.id as u8).count());
                    println!(
                        "    target {} — ({:.2}, {:.2}, {:.2}) m, range {:.2} m, speed {:.2} m/s, confidence {:.2}, {} point(s)",
                        target.id,
                        target.x,
                        target.y,
                        target.z,
                        target.range_m(),
                        target.speed_mps(),
                        target.confidence,
                        assigned.map_or_else(|| "?".to_string(), |count| count.to_string()),
                    );
                }
                #[cfg(feature = "vital-signs")]
                if !frame.previous_point_associations.is_empty() {
                    let unassigned = frame
                        .previous_point_associations
                        .iter()
                        .filter(|association| association.target_id().is_none())
                        .count();
                    println!(
                        "    associations — {} for the previous frame's {} points, {} unassigned",
                        frame.previous_point_associations.len(),
                        previous.as_ref().map_or_else(
                            || "?".to_string(),
                            |earlier| earlier.points.len().to_string()
                        ),
                        unassigned,
                    );
                }
                #[cfg(feature = "vital-signs")]
                for reading in &frame.vital_signs {
                    println!(
                        "    vitals — subject {} at range bin {}, heart {:.1} bpm, breath {:.1} bpm, breath deviation {:.3}",
                        reading.subject_id,
                        reading.range_bin,
                        reading.heart_rate_bpm,
                        reading.breathing_rate_bpm,
                        reading.breathing_deviation,
                    );
                }
                if !frame.unknown_tlv_types.is_empty() {
                    println!("    unknown tlv types: {:?}", frame.unknown_tlv_types);
                }
                #[cfg(feature = "vital-signs")]
                {
                    previous = Some(frame);
                }
            }
            Ok(None) => {
                println!("data_probe: stream ended");
                break;
            }
            Err(error) => {
                errors += 1;
                eprintln!("  parse error: {error}");
                if errors > 5 {
                    return ExitCode::FAILURE;
                }
            }
        }
    }
    println!("data_probe: {seen} frame(s) parsed, {errors} error(s)");
    ExitCode::SUCCESS
}
