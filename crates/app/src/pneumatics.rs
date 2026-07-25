// SPDX-License-Identifier: Apache-2.0

//! The CA35-side pneumatics: **two symmetric sections**, driven identically.
//!
//! Both hang off Linux-owned timers, because TIM4 and TIM5 are the only
//! PWM-capable timers whose channels reach the DK's 40-pin connector and the
//! board's RIF configuration reaches them only from the AP:
//!
//! | Section | Timer      | Pin   | Connector | sysfs           | Driver        |
//! | ------- | ---------- | ----- | --------- | --------------- | ------------- |
//! | A       | `TIM4_CH2` | `PA1` | pin 33    | `pwmchip4/pwm1` | FR120N module |
//! | B       | `TIM5_CH1` | `PH8` | pin 31    | `pwmchip8/pwm0` | FR120N module |
//!
//! The two are interchangeable: same part, same frequency, same duty, same
//! contract. Nothing in this module — or above it — distinguishes them, and
//! [`Pneumatics`] exists mostly to make that hard to get wrong by accident, since
//! a mat whose halves disagree is a mat that tips whatever is resting on it.
//!
//! Each timer's `pwm-stm32` provider appears as its own `/sys/class/pwm/pwmchipN`.
//! Those chip indices are assigned in probe order and are **not** stable across
//! kernel or device-tree changes, so [`PneumaticConfig`] carries them rather than
//! this module hard-coding them — and the deployment fills it in from the
//! `[pneumatics]` section of `Repose.toml` beside the binary (see
//! [`crate::config`]), so correcting one is an edit on the board rather than a
//! cross-build. See `hardware/pneumatics/README.md` for how to re-identify them
//! on a live board.
//!
//! # Independent sections
//!
//! Two channels that always carry the same value are, today, an expensive way to
//! send one value. They are wired and driven separately anyway because the
//! asymmetry is the obvious next feature — a cradle that rises under one forearm
//! before the other, or a canopy that leans away from the window — and retrofitting
//! a second channel is harder than not collapsing them now. Anything that grows
//! that direction changes [`PneumaticState::Cycle`] to carry a duty per section;
//! the driver plumbing below is already per-section.
//!
//! # Fail-safe
//!
//! Each section's valve is assumed **normally open**: de-energized, that section
//! vents to atmosphere; energized, it seals so air can build. That polarity is
//! what makes losing control fail safe, which matters here because this design
//! has no independent interlock — the CM33 is busy owning the radar UART and
//! holds no actuator line at all. It matters twice over now that the lines are
//! cycled rather than parked: a stalled control loop leaves the last duty
//! running, but a *dead* one drops both lines, and a dropped line is a fully open
//! valve. So every path that ends the process must end with both outputs low:
//!
//! * [`Pneumatics::set_state`] to [`PneumaticState::Vent`] on any shutdown;
//! * [`crate::sysfs_pwm::SysfsPwm`]'s `Drop` disables and unexports the channel,
//!   which covers a clean exit and a panic that unwinds;
//! * the pins idle low after the driver releases them, which covers a kill.
//!
//! A hard `SIGKILL` mid-inflation is the one case the software cannot cover, and
//! even there the pins idle low. What bounds the pressure the sections can be
//! holding when that happens is the per-mode budgets and the `max_charge`
//! ceiling in [`InflationParams`](snf_bridge::inflation::InflationParams).

use std::io;

use crate::fr120n::{self, Fr120n};
use crate::sysfs_pwm::SysfsPwm;

/// Default section PWM frequency, in Hz — **the breath carrier**, and the one
/// frequency here that is a hard constraint rather than a preference. Both
/// sections run it, and a deployment can override it through
/// [`PneumaticConfig::pwm_hz`].
///
/// The valve is not switched at a rate the solenoid cannot follow; it is
/// switched at a rate it *must* follow, because its duty is what sets the
/// intake/exhaust ratio (see [`PneumaticState`]). The armature has to complete a
/// full open-close cycle every period, which puts the usable range at **20–50
/// Hz**: below it the pulsing becomes palpable and audible as individual clicks,
/// above it the valve cannot keep up and the duty stops meaning anything — it
/// stays part-open, and the mapping from duty to net flow quietly goes
/// non-monotonic.
///
/// 40 Hz sits toward the top of that range: fast enough that a 25 ms cycle is
/// felt as continuous pressure rather than as individual pulses, with margin to
/// drop toward 20 Hz if the valves on the bench turn out to be slower. Both
/// sections must use the same value — two halves breathing at different rates is
/// exactly the beat frequency a person would notice.
pub const SECTION_PWM_HZ: u32 = 40;

/// Which `/sys/class/pwm/pwmchipN` and channel backs each section, and at what
/// frequency both run.
///
/// The defaults are what the DK enumerates: `TIM4_CH2` is `pwmchip4/pwm1` and
/// `TIM5_CH1` is `pwmchip8/pwm0`. Note that `pwm-stm32` numbers channels by the
/// **timer's own** channel index — `CH1` is `pwm0`, `CH2` is `pwm1` — regardless
/// of how many the device tree exposes, which is why section A sits on channel 1
/// of a chip whose other channels are unused.
///
/// Swapping the two halves of this struct swaps which physical section is called
/// A. Nothing depends on the answer while both are driven identically.
///
/// A deployment supplies this through the `[pneumatics]` section of
/// `Repose.toml` — see [`crate::config::PneumaticsSection`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PneumaticConfig {
    /// Carrier frequency for **both** sections, in Hz. There is deliberately one
    /// field rather than one per section: see [`SECTION_PWM_HZ`].
    pub pwm_hz: u32,
    /// `pwmchip` index of the TIM4 provider (section A).
    pub section_a_chip: u32,
    /// Channel within section A's chip: `1` for `TIM4_CH2`.
    pub section_a_channel: u32,
    /// `pwmchip` index of the TIM5 provider (section B).
    pub section_b_chip: u32,
    /// Channel within section B's chip: `0` for `TIM5_CH1`.
    pub section_b_channel: u32,
}

impl Default for PneumaticConfig {
    fn default() -> Self {
        Self {
            pwm_hz: SECTION_PWM_HZ,
            section_a_chip: 4,
            section_a_channel: 1,
            section_b_chip: 8,
            section_b_channel: 0,
        }
    }
}

/// What the pneumatics should be doing.
///
/// There are only two states, because the actuator has **no equilibrium**: while
/// a section runs, every period both inhales (line high, valve sealed, air
/// pushed in) and exhales (line low, valve open, section pushing air out). The
/// duty is therefore not a throttle but the ratio between the two, and it is the
/// *net* of them that inflates or deflates:
///
/// ```text
///   duty 0 ─────────── neutral ─────────── duty 100
///   all exhaust      in ≈ out           all intake
/// ```
///
/// "Hold" is not a state this hardware has; it is whatever duty happens to
/// balance supply against the valve's orifice, which only a bench can measure
/// (`snf_bridge::inflation::InflationParams::neutral_duty`). Both sections are
/// assumed to balance at the same duty — that is what "symmetric" has to mean to
/// be useful. If the bench says otherwise, the fix is a per-section trim here,
/// not a different neutral in the control model.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PneumaticState {
    /// Nothing driven: both lines low, both sections exhausting through their
    /// open valves. The resting state, and the state every shutdown path drives
    /// to.
    #[default]
    Vent,
    /// Both sections cycling at this duty (`0..=100`) at [`SECTION_PWM_HZ`].
    Cycle(u8),
}

/// The two sections as one unit, so they can never be left disagreeing.
///
/// The pair is driven, and fails, together: a write that lands on one section
/// and not the other leaves the mat lopsided, so [`Pneumatics::set_state`]
/// reports that as an error and the caller vents rather than continuing on one
/// half.
pub struct Pneumatics {
    section_a: Fr120n<SysfsPwm>,
    section_b: Fr120n<SysfsPwm>,
    state: PneumaticState,
}

impl Pneumatics {
    /// Export and configure both PWM channels, leaving both sections venting.
    ///
    /// Both take `config.pwm_hz`: one value, read once, so there is no path by
    /// which the two halves end up breathing at different rates.
    pub fn open(config: PneumaticConfig) -> io::Result<Self> {
        let section_a = SysfsPwm::new(
            config.section_a_chip,
            config.section_a_channel,
            config.pwm_hz,
        )?;
        let section_b = SysfsPwm::new(
            config.section_b_chip,
            config.section_b_channel,
            config.pwm_hz,
        )?;

        Ok(Self {
            section_a: Fr120n::new(section_a),
            section_b: Fr120n::new(section_b),
            state: PneumaticState::Vent,
        })
    }

    /// The last state successfully applied.
    pub fn state(&self) -> PneumaticState {
        self.state
    }

    /// Drive both sections to `state`.
    ///
    /// The two writes cannot be made atomic — they are separate timers and
    /// separate sysfs files — so the sections are briefly a duty apart on every
    /// change. At 40 Hz that gap is far shorter than the pneumatics' own
    /// response, so it is invisible in the air; what matters is that a *failed*
    /// write is not left standing. If the second write fails, the first is rolled
    /// back to vent before returning, so the pair is never parked mismatched.
    pub fn set_state(&mut self, state: PneumaticState) -> Result<(), fr120n::Error> {
        let result = match state {
            PneumaticState::Vent => Self::apply(&mut self.section_a, &mut self.section_b, None),
            PneumaticState::Cycle(duty) => {
                Self::apply(&mut self.section_a, &mut self.section_b, Some(duty))
            }
        };

        match result {
            Ok(()) => {
                self.state = state;
                Ok(())
            }
            Err(e) => {
                // One section may be running while the other is not. Neither is
                // a state anyone asked for, so drop both and let the caller
                // decide whether to keep actuating.
                let _ = Self::apply(&mut self.section_a, &mut self.section_b, None);
                self.state = PneumaticState::Vent;
                Err(e)
            }
        }
    }

    /// Write one duty (or nothing, to vent) to both sections.
    fn apply(
        section_a: &mut Fr120n<SysfsPwm>,
        section_b: &mut Fr120n<SysfsPwm>,
        duty: Option<u8>,
    ) -> Result<(), fr120n::Error> {
        match duty {
            None => {
                section_a.off()?;
                section_b.off()?;
            }
            Some(duty) => {
                section_a.set_percent(duty)?;
                section_b.set_percent(duty)?;
            }
        }
        Ok(())
    }

    /// Whether the sections are being driven at all.
    pub fn running(&self) -> bool {
        self.section_a.is_on() || self.section_b.is_on()
    }
}

impl Drop for Pneumatics {
    fn drop(&mut self) {
        // Best-effort: vent before the channels are released. `SysfsPwm`'s own
        // `Drop` disables and unexports each channel right after, which drives
        // both pins low regardless of whether these writes landed.
        let _ = self.set_state(PneumaticState::Vent);
    }
}
