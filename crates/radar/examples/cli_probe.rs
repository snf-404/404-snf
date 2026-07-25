// SPDX-License-Identifier: Apache-2.0

//! Talk to an IWR6843's configuration CLI and report what it says.
//!
//! The bring-up question this answers is "does the sensor hear us at all", the
//! one that has to be settled before a silent data port means anything. It is
//! the same [`RadarCli`] `snf-app` uses, so a probe that works and an
//! application that does not is a difference in configuration, not in code.
//!
//! ```text
//! cli_probe /dev/ttyUSB0                  # sensorStop + flushCfg only
//! cli_probe /dev/ttyUSB0 --builtin        # the whole built-in vital-signs profile
//! cli_probe /dev/ttyUSB0 vital-signs.cfg  # a profile from a file
//! ```
//!
//! With no profile argument it sends only the reset preamble every profile
//! starts with, which leaves the sensor stopped and its configuration flushed —
//! safe against firmware whose profile you do not have.

use std::{env, process::ExitCode};

use snf_radar::{RadarCli, RadarCliConfig, SensorProfile};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let mut arguments = env::args().skip(1);
    let Some(port) = arguments.next() else {
        eprintln!("usage: cli_probe <port> [--builtin | <profile.cfg>]");
        return ExitCode::FAILURE;
    };
    let profile = match arguments.next().as_deref() {
        None => SensorProfile::parse("sensorStop\nflushCfg\n"),
        Some("--builtin") => SensorProfile::builtin(),
        Some(path) => match SensorProfile::load(path) {
            Ok(profile) => profile,
            Err(error) => {
                eprintln!("cli_probe: {error}");
                return ExitCode::FAILURE;
            }
        },
    };

    let config = RadarCliConfig {
        cli_port: port,
        ..RadarCliConfig::default()
    };
    println!(
        "cli_probe: {} @ {}, {} command(s)",
        config.cli_port,
        config.baud_rate,
        profile.len()
    );

    let mut cli = match RadarCli::open(&config) {
        Ok(cli) => cli,
        Err(error) => {
            eprintln!("cli_probe: {error}");
            return ExitCode::FAILURE;
        }
    };
    match cli.apply(&profile).await {
        Ok(report) => {
            for note in &report.notes {
                println!("cli_probe:   {note}");
            }
            println!("cli_probe: {} command(s) accepted", report.commands);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("cli_probe: {error}");
            ExitCode::FAILURE
        }
    }
}
