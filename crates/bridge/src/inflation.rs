// SPDX-License-Identifier: Apache-2.0

//! Fatigue → inflation-speed control: the piecewise, hysteretic model behind the
//! mat's three deformation modes.
//!
//! A fatigue verdict is a level in `0..=100`; the pneumatics take one duty.
//! Mapping one to the other linearly would make the mat lurch in step with a
//! noisy classifier, so the relationship is deliberately *not* monotone in
//! fatigue. It is piecewise by mode, with a hysteresis loop between modes:
//!
//! | Mode     | Fatigue `F`      | Speed profile                     | Budget |
//! | -------- | ---------------- | --------------------------------- | ------ |
//! | `Idle`   | `F < 30`         | vent                              | —      |
//! | `Nudge`  | `30 <= F < 60`   | sigmoid micro-rise toward `v_min` | 10 s   |
//! | `Cradle` | `60 <= F < 85`   | inverted-U, peak at `F_optimal`   | 5 s    |
//! | `Canopy` | `F >= 85`, armed | constant `v_canopy`               | 15 s   |
//!
//! Once a mode's budget is spent the deformation is complete: `Nudge` and
//! `Canopy` settle at net-neutral, and `Cradle` crosses into a breathing steady
//! state — a slow sinusoid about neutral, `v(t) = A sin(ωt)`, 7 s per cycle.
//!
//! # What "speed" means here
//!
//! The actuator has **no equilibrium state**. Within every PWM period a section
//! inhales while its line is high and exhales while it is low, so the duty is
//! not a throttle — it is the ratio between the two, and the *net* flow is what
//! the duty selects:
//!
//! ```text
//!   duty 0 ─────────── neutral ─────────── duty 100
//!   all exhaust      in ≈ out           all intake
//!   ← deflating       holding            inflating →
//! ```
//!
//! So speed here is a **signed, normalized command in `-1.0..=1.0`**: `+1` is the
//! fastest inflation the loop will command ([`InflationParams::duty_ceiling`]),
//! `0` is [`InflationParams::neutral_duty`], and `-1` is a full exhaust. Both
//! halves of a breath are therefore directly expressible.
//!
//! One duty is produced per tick and both of the mat's symmetric sections are
//! driven from it — see `snf-app`'s `pneumatics` module. Nothing in this model
//! knows how many sections there are, which is what would make per-section
//! deformation (a cradle that rises under one arm first) a change confined to
//! what carries the command, not to how it is computed.
//!
//! `neutral_duty` is the one number in here that only a bench can supply: it is
//! wherever a section's supply and its valve's orifice happen to balance. The
//! default is a placeholder, not a measurement — everything below is *relative*
//! to it, so calibrating it is the first thing to do on real hardware, and both
//! sections are assumed to land on the same value.
//!
//! The model is written in kPa/s, but this board has no pressure feedback (the
//! MPRLS breakout in `hardware/pneumatics/README.md` is specified and not wired),
//! so speed is open-loop: monotone in real flow, not calibrated to it. Closing
//! that loop is what turns these into true kPa/s.
//!
//! # Fail-safe accounting
//!
//! With no pressure sensor and no independent interlock (the CM33 holds no
//! actuator line), the bound on how much air can accumulate in a section is
//! arithmetic in this module:
//!
//! * each mode may inflate for at most its budget, granted **once per episode**
//!   — re-entering a mode the episode already passed through grants nothing, so
//!   fatigue oscillating across a boundary cannot ratchet the sections up;
//! * [`InflationParams::max_charge`] caps an episode's *net* delivered air,
//!   `∫ (duty − neutral) dt` in duty-seconds, after which the controller will
//!   only ever hold or exhale. Because the ledger is signed, an exhaling breath
//!   pays back what an inhaling one spent and the steady state does not drift
//!   into the ceiling;
//! * a verdict older than [`InflationParams::verdict_timeout`] pins the command
//!   to neutral, and one twice that old ends the episode and vents.
//!
//! An episode ends — and all of that resets — only on a full release to
//! [`InflationMode::Idle`].

use std::time::{Duration, Instant};

/// Which deformation the mat is performing.
///
/// Ordered by how much structure the mode implies, so promotion and demotion are
/// plain comparisons.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum InflationMode {
    /// Vented and resting.
    #[default]
    Idle,
    /// A tactile "tap": a rise near the perception threshold, easy to ignore.
    Nudge,
    /// A bowl that catches resting forearms, then breathes.
    Cradle,
    /// A slow overhead boundary. Requires an explicit user trigger.
    Canopy,
}

/// What the pneumatics should do this tick.
///
/// Mirrors what the hardware can actually be asked for, without depending on
/// `snf-app`'s types; the application maps it onto its `PneumaticState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actuation {
    /// At rest: nothing driving the line, so the section exhausts through the
    /// de-energized valve. The state every shutdown path lands in.
    Vent,
    /// Cycling at this duty (`0..=100`): inhaling above
    /// [`InflationParams::neutral_duty`], exhaling below it, and — as close as
    /// this actuator comes to holding — neither at it.
    Cycle(u8),
}

/// One tick's decision, with the reasoning attached for telemetry and logs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InflationCommand {
    /// What to drive the pneumatics to.
    pub actuation: Actuation,
    /// The mode that produced it.
    pub mode: InflationMode,
    /// Commanded normalized speed, `-1.0..=1.0`. Negative is a net exhale.
    pub speed: f32,
}

/// Tunables for the piecewise model.
///
/// The defaults are the specified operating point; everything is public so a
/// bench with different sections, supply or valves can be re-derated without
/// touching the control logic.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InflationParams {
    /// `F` at or above which `Nudge` engages.
    pub nudge_on: f32,
    /// `F` at or above which `Cradle` engages.
    pub cradle_on: f32,
    /// `F` at or above which `Canopy` engages — and only when armed.
    pub canopy_on: f32,
    /// How far below a mode's entry threshold `F` must fall to release it. The
    /// gap between `F_on` and `F_off` is what a brief look-up cannot cross.
    pub release_margin: f32,
    /// How long `F` must stay below `F_off` continuously before releasing.
    pub release_dwell: Duration,

    /// Ceiling of the `Nudge` sigmoid: `v(F) = v_min · σ(k(F − F_mid))`.
    pub v_min: f32,
    /// Sigmoid steepness `k`.
    pub sigmoid_k: f32,
    /// Sigmoid midpoint `F_mid`. Defaults to the middle of the nudge band.
    pub sigmoid_mid: f32,

    /// Peak of the `Cradle` parabola: `v(F) = v_target − a(F − F_optimal)²`.
    pub v_target: f32,
    /// `F_optimal`, where the parabola peaks. Defaults to the middle of the
    /// cradle band.
    pub cradle_optimal: f32,
    /// Speed at the *edges* of the cradle band. `a` is derived from this and
    /// `v_target` rather than being tuned directly, so the curve always meets
    /// the neighbouring phases at a defined speed instead of an arbitrary one.
    pub v_cradle_edge: f32,

    /// The fixed `Canopy` speed. Decoupled from `F` on purpose: a structure
    /// deploying overhead should not accelerate because the person under it got
    /// more tired.
    pub v_canopy: f32,

    /// Derivative gain `α` in `v = f(F) + α·dF/dt`, per (level/s). Applied to
    /// `Nudge` and `Cradle` only.
    pub derivative_gain: f32,
    /// Clamp on the derivative term's contribution, so a jumpy verdict cannot
    /// dominate `f(F)`.
    pub derivative_limit: f32,

    /// Breathing period in the cradle steady state (`2π/ω`).
    pub breath_period: Duration,
    /// Breathing amplitude `A`, in normalized speed. The breath swings
    /// symmetrically about neutral, so it inhales and exhales the same amount
    /// and the steady state neither drifts up nor sags.
    pub breath_amplitude: f32,

    /// Duty at which intake balances exhaust — net zero flow.
    ///
    /// **Bench-calibrated.** It is set by a section's supply against its valve's
    /// orifice; the default is a placeholder. Too low and a "hold" slowly
    /// deflates, too high and it slowly inflates — the charge ceiling bounds the
    /// second case, nothing bounds the first but comfort.
    ///
    /// Still uncalibrated as of the bench build: the powered pump was missed in
    /// the parts order and the rig is hand-pumped, which has no steady state to
    /// measure against. See `hardware/pneumatics/README.md` § "As built".
    pub neutral_duty: u8,
    /// Highest duty the loop will command. Deliberately short of 100 so that
    /// even the fastest inflation still exhales a little of every cycle, rather
    /// than becoming a sealed section with no way out but the ceiling.
    pub duty_ceiling: u8,

    /// Inflation budget granted on first entry to `Nudge`.
    pub nudge_budget: Duration,
    /// Inflation budget granted on first entry to `Cradle`.
    pub cradle_budget: Duration,
    /// Inflation budget granted on first entry to `Canopy`.
    pub canopy_budget: Duration,

    /// Hard ceiling on one episode's *net* delivered air, in duty-seconds
    /// (`∫ (duty − neutral) dt`, duty as a fraction). Past it the controller
    /// will only hold or exhale.
    ///
    /// The default is roughly the three budgets at their nominal speeds
    /// (`≈ 0.4 + 1.6 + 0.9`) with a little headroom. Re-derive it against the
    /// real sections before power-on: with no pressure sensor in the loop, this
    /// number and the valve's normally-open polarity are the whole of the
    /// over-inflation protection.
    ///
    /// It is a proxy for delivered air only while supply is roughly constant, so
    /// the hand-pumped bench build does not exercise it at all — this ceiling is
    /// currently **untested**. Deriving it is the thing to do first once a
    /// powered pump is in line.
    pub max_charge: f32,

    /// A verdict older than this pins the command to neutral; twice this ends
    /// the episode.
    pub verdict_timeout: Duration,
}

impl Default for InflationParams {
    fn default() -> Self {
        Self {
            nudge_on: 30.0,
            cradle_on: 60.0,
            canopy_on: 85.0,
            release_margin: 8.0,
            release_dwell: Duration::from_secs(5),

            v_min: 0.20,
            sigmoid_k: 0.25,
            sigmoid_mid: 45.0,

            v_target: 1.0,
            cradle_optimal: 72.5,
            v_cradle_edge: 0.35,

            v_canopy: 0.15,

            derivative_gain: 0.02,
            derivative_limit: 0.25,

            breath_period: Duration::from_secs(7),
            breath_amplitude: 0.12,

            neutral_duty: 50,
            duty_ceiling: 90,

            nudge_budget: Duration::from_secs(10),
            cradle_budget: Duration::from_secs(5),
            canopy_budget: Duration::from_secs(15),

            max_charge: 3.5,

            verdict_timeout: Duration::from_secs(3),
        }
    }
}

/// How strongly a new `dF/dt` sample displaces the running estimate. Fatigue
/// verdicts arrive at the vitals rate and are noisy; the raw difference of two of
/// them is noisier still.
const RATE_SMOOTHING: f32 = 0.3;

/// Longest tick the budget/charge integrators will credit, so a stalled loop
/// cannot retire a whole budget in one step.
const MAX_TICK: Duration = Duration::from_millis(500);

/// The fatigue → inflation control loop.
///
/// Two entry points, matching the application's two event sources: [`observe`]
/// when a fatigue verdict arrives (at the vitals rate), and [`command`] on the
/// actuator tick. Keeping them apart is what lets the budgets and the release
/// dwell keep running when the radar goes quiet mid-inflation.
///
/// [`observe`]: InflationController::observe
/// [`command`]: InflationController::command
#[derive(Clone, Debug)]
pub struct InflationController {
    params: InflationParams,
    mode: InflationMode,
    /// Highest mode this episode has reached; budgets are granted against it.
    peak: InflationMode,
    /// Last observed fatigue level.
    level: f32,
    /// Smoothed `dF/dt`, levels per second.
    rate: f32,
    last_observation: Option<(Instant, f32)>,
    last_tick: Option<Instant>,
    /// When `F` first fell below the current mode's release threshold.
    release_since: Option<Instant>,
    /// Inflation time left in the current mode.
    budget: Duration,
    /// Net air delivered this episode, in duty-seconds.
    charge: f32,
    /// Phase origin of the cradle's breathing steady state.
    steady_since: Option<Instant>,
    /// Whether the user has explicitly asked for `Canopy`.
    canopy_armed: bool,
    /// Last commanded speed, for telemetry.
    speed: f32,
}

impl Default for InflationController {
    fn default() -> Self {
        Self::new(InflationParams::default())
    }
}

impl InflationController {
    /// A controller resting at [`InflationMode::Idle`] with `Canopy` disarmed.
    pub fn new(params: InflationParams) -> Self {
        Self {
            params,
            mode: InflationMode::Idle,
            peak: InflationMode::Idle,
            level: 0.0,
            rate: 0.0,
            last_observation: None,
            last_tick: None,
            release_since: None,
            budget: Duration::ZERO,
            charge: 0.0,
            steady_since: None,
            canopy_armed: false,
            speed: 0.0,
        }
    }

    /// The mode last commanded.
    pub fn mode(&self) -> InflationMode {
        self.mode
    }

    /// The normalized speed last commanded.
    pub fn speed(&self) -> f32 {
        self.speed
    }

    /// Net air delivered this episode, in duty-seconds. Resets on a full
    /// release.
    pub fn charge(&self) -> f32 {
        self.charge
    }

    /// Arm or disarm [`InflationMode::Canopy`].
    ///
    /// Canopy is the one mode fatigue alone must never reach: a structure that
    /// closes over someone's head is only acceptable when they asked for it.
    /// Disarming mid-canopy demotes on the next tick but does **not** vent — the
    /// hysteresis loop still owns release.
    pub fn arm_canopy(&mut self, armed: bool) {
        self.canopy_armed = armed;
        if !armed && self.mode == InflationMode::Canopy {
            self.mode = InflationMode::Cradle;
            self.budget = Duration::ZERO;
        }
    }

    /// Feed in a fatigue verdict.
    ///
    /// Updates `F` and `dF/dt`, promotes immediately when `F` crosses a mode's
    /// entry threshold, and starts (or clears) the release dwell. Demotion is not
    /// applied here — the dwell has to expire first, which [`command`] checks.
    ///
    /// [`command`]: InflationController::command
    pub fn observe(&mut self, now: Instant, level: u8) {
        let level = f32::from(level);

        if let Some((at, previous)) = self.last_observation {
            let dt = now.saturating_duration_since(at).as_secs_f32();
            if dt > 1e-3 {
                let sample = (level - previous) / dt;
                self.rate += (sample - self.rate) * RATE_SMOOTHING;
            }
        }
        self.last_observation = Some((now, level));
        self.level = level;

        // Rising: engage the new mode at once. Falling: only arm the dwell —
        // `command` decides whether it survives long enough to release.
        let rising = self.mode_at(level, self.params.nudge_on);
        if rising > self.mode {
            self.enter(rising);
            self.release_since = None;
            return;
        }

        let falling = self.mode_at(level, self.release_base());
        if falling < self.mode {
            self.release_since.get_or_insert(now);
        } else {
            self.release_since = None;
        }
    }

    /// Decide what the pneumatics should be doing now.
    ///
    /// Called on the actuator tick, independently of the radar, so that budgets,
    /// the charge ceiling, the release dwell and the staleness checks all keep
    /// advancing when verdicts stop arriving.
    pub fn command(&mut self, now: Instant) -> InflationCommand {
        let dt = self.tick(now);

        // ── Signal loss ──────────────────────────────────────────────────────
        // Stale data must never buy more air. Briefly stale pins the command to
        // neutral; properly gone ends the episode.
        let Some((observed_at, _)) = self.last_observation else {
            return self.idle();
        };
        let age = now.saturating_duration_since(observed_at);
        if age >= self.params.verdict_timeout.saturating_mul(2) {
            self.release();
            return self.idle();
        }
        if age >= self.params.verdict_timeout {
            return self.neutral();
        }

        // ── Hysteresis: has the release dwell run out? ───────────────────────
        if let Some(since) = self.release_since
            && now.saturating_duration_since(since) >= self.params.release_dwell
        {
            let target = self.mode_at(self.level, self.release_base());
            self.enter(target);
            self.release_since = None;
        }

        match self.mode {
            InflationMode::Idle => self.idle(),
            InflationMode::Nudge => {
                if self.budget.is_zero() {
                    // The tap has landed; keeping it there is the whole point.
                    self.neutral()
                } else {
                    let speed = self.nudge_speed() + self.derivative_term();
                    self.drive(speed, dt)
                }
            }
            InflationMode::Cradle => {
                if self.budget.is_zero() {
                    self.breathe(now, dt)
                } else {
                    let speed = self.cradle_speed() + self.derivative_term();
                    self.drive(speed, dt)
                }
            }
            InflationMode::Canopy => {
                if self.budget.is_zero() {
                    self.neutral()
                } else {
                    // No derivative term: Phase III is decoupled from `F`.
                    self.drive(self.params.v_canopy, dt)
                }
            }
        }
    }

    // ── Speed curves ─────────────────────────────────────────────────────────

    /// Phase I — sigmoidal slow rise, ceiling `v_min`.
    fn nudge_speed(&self) -> f32 {
        let p = &self.params;
        p.v_min / (1.0 + (-p.sigmoid_k * (self.level - p.sigmoid_mid)).exp())
    }

    /// Phase II — inverted U peaking at `F_optimal`.
    ///
    /// `a` comes from the requirement that the curve pass through
    /// `v_cradle_edge` at the band's lower edge, so the accelerate-then-decelerate
    /// shape is fixed by the two speeds that matter rather than by a bare
    /// coefficient.
    fn cradle_speed(&self) -> f32 {
        let p = &self.params;
        let half_width = p.cradle_optimal - p.cradle_on;
        let a = if half_width.abs() < f32::EPSILON {
            0.0
        } else {
            (p.v_target - p.v_cradle_edge) / (half_width * half_width)
        };
        let offset = self.level - p.cradle_optimal;
        (p.v_target - a * offset * offset).max(p.v_cradle_edge.min(p.v_target))
    }

    /// `α·dF/dt`, clamped.
    fn derivative_term(&self) -> f32 {
        let limit = self.params.derivative_limit;
        (self.params.derivative_gain * self.rate).clamp(-limit, limit)
    }

    /// The cradle's steady state: a slow sinusoid about neutral.
    ///
    /// This is the one place the actuator's lack of an equilibrium is an asset —
    /// the same duty that inhales above neutral exhales below it, so a breath is
    /// literally `A sin(ωt)` and needs no separate deflate path. Symmetry is also
    /// what keeps it sustainable: each cycle returns to the charge ledger what it
    /// borrowed, so the steady state can run indefinitely without walking into
    /// the ceiling.
    /// The breath is built in *duty* rather than in normalized speed, and swings
    /// by the same number of duty points either way. Going through
    /// [`Self::duty_for`] would not: `+1` reaches the ceiling while `-1` reaches
    /// zero, and unless neutral sits exactly halfway between them the two halves
    /// are unequal — enough, at these amplitudes, for the steady state to walk
    /// off over a few minutes. Equal duty excursion is the best available proxy
    /// for equal air, and only the pressure loop can make it exact.
    fn breathe(&mut self, now: Instant, dt: Duration) -> InflationCommand {
        let origin = *self.steady_since.get_or_insert(now);
        let p = self.params;
        let period = p.breath_period.as_secs_f32().max(f32::EPSILON);
        let phase =
            std::f32::consts::TAU * now.saturating_duration_since(origin).as_secs_f32() / period;
        let swing = phase.sin();

        let neutral = f32::from(p.neutral_duty);
        let span = p.breath_amplitude * (f32::from(p.duty_ceiling) - neutral).min(neutral);
        let duty = (neutral + span * swing).round().clamp(0.0, 100.0) as u8;

        self.drive_duty(duty, p.breath_amplitude * swing, dt)
    }

    // ── Mechanics ────────────────────────────────────────────────────────────

    /// The base the *falling* thresholds are measured from: every entry
    /// threshold lowered by the release margin, so `F_off < F_on` and the gap
    /// between them is the band a transient cannot cross.
    fn release_base(&self) -> f32 {
        self.params.nudge_on - self.params.release_margin
    }

    /// Which mode `level` calls for, with every threshold shifted by
    /// `base - nudge_on`. Passing `nudge_on` gives the entry (rising)
    /// thresholds; passing [`Self::release_base`] gives the release (falling)
    /// ones.
    fn mode_at(&self, level: f32, base: f32) -> InflationMode {
        let p = &self.params;
        let shift = base - p.nudge_on;
        if level >= p.canopy_on + shift && self.canopy_armed {
            InflationMode::Canopy
        } else if level >= p.cradle_on + shift {
            InflationMode::Cradle
        } else if level >= p.nudge_on + shift {
            InflationMode::Nudge
        } else {
            InflationMode::Idle
        }
    }

    /// Move to `mode`, granting a budget only for ground the episode has not
    /// already covered.
    fn enter(&mut self, mode: InflationMode) {
        if mode == InflationMode::Idle {
            self.release();
            return;
        }
        if mode < self.mode {
            // Demoting means less structure is wanted, so whatever was left of
            // the higher mode's budget is forfeit — it must not be spent at the
            // lower mode's speed on the way down.
            self.budget = Duration::ZERO;
        } else if mode > self.peak {
            self.budget = match mode {
                InflationMode::Nudge => self.params.nudge_budget,
                InflationMode::Cradle => self.params.cradle_budget,
                InflationMode::Canopy => self.params.canopy_budget,
                InflationMode::Idle => Duration::ZERO,
            };
            self.peak = mode;
        }
        if mode == InflationMode::Cradle && self.mode != InflationMode::Cradle {
            // Re-entering the cradle restarts the breath rather than resuming a
            // phase from minutes ago; the origin is set when the steady state is
            // actually reached.
            self.steady_since = None;
        }
        self.mode = mode;
    }

    /// End the episode: vent, and give back every budget and the charge ledger.
    fn release(&mut self) {
        self.mode = InflationMode::Idle;
        self.peak = InflationMode::Idle;
        self.budget = Duration::ZERO;
        self.charge = 0.0;
        self.steady_since = None;
        self.release_since = None;
        self.speed = 0.0;
    }

    /// Time since the last tick, clamped so a stalled loop cannot spend a whole
    /// budget in one step.
    fn tick(&mut self, now: Instant) -> Duration {
        let dt = self
            .last_tick
            .map(|last| now.saturating_duration_since(last).min(MAX_TICK))
            .unwrap_or(Duration::ZERO);
        self.last_tick = Some(now);
        dt
    }

    /// Command `speed`, spending budget and charge for it.
    ///
    /// Inhaling past the episode's charge ceiling is refused — the command falls
    /// back to neutral — but exhaling is always allowed, and pays back into the
    /// ledger, so a breath that has hit the ceiling keeps its down-swing and
    /// recovers room for the next up-swing.
    fn drive(&mut self, speed: f32, dt: Duration) -> InflationCommand {
        let speed = speed.clamp(-1.0, 1.0);
        self.drive_duty(self.duty_for(speed), speed, dt)
    }

    /// [`Self::drive`] once the duty is already decided, for callers that build
    /// it themselves. `speed` is carried through for telemetry and to say which
    /// way this command is going.
    fn drive_duty(&mut self, duty: u8, speed: f32, dt: Duration) -> InflationCommand {
        if speed > 0.0 && self.charge >= self.params.max_charge {
            return self.neutral();
        }

        if speed > 0.0 {
            self.budget = self.budget.saturating_sub(dt);
        }
        let net = (f32::from(duty) - f32::from(self.params.neutral_duty)) / 100.0;
        self.charge = (self.charge + net * dt.as_secs_f32()).max(0.0);
        self.speed = speed;
        InflationCommand {
            actuation: Actuation::Cycle(duty),
            mode: self.mode,
            speed,
        }
    }

    /// Map a signed normalized speed onto a duty: `0` is neutral, `+1` is
    /// [`InflationParams::duty_ceiling`], `-1` is a full exhaust.
    fn duty_for(&self, speed: f32) -> u8 {
        let neutral = f32::from(self.params.neutral_duty);
        let speed = speed.clamp(-1.0, 1.0);
        let duty = if speed >= 0.0 {
            let ceiling = f32::from(self.params.duty_ceiling).max(neutral);
            neutral + speed * (ceiling - neutral)
        } else {
            neutral + speed * neutral
        };
        duty.round().clamp(0.0, 100.0) as u8
    }

    /// Cycle at neutral: as close to holding as an actuator with no equilibrium
    /// state gets.
    fn neutral(&mut self) -> InflationCommand {
        self.speed = 0.0;
        InflationCommand {
            actuation: Actuation::Cycle(self.params.neutral_duty),
            mode: self.mode,
            speed: 0.0,
        }
    }

    fn idle(&mut self) -> InflationCommand {
        self.speed = 0.0;
        InflationCommand {
            actuation: Actuation::Vent,
            mode: InflationMode::Idle,
            speed: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the controller for `duration` at the application's 100 ms actuator
    /// tick, holding `level` steady, and return the last command.
    fn run(
        controller: &mut InflationController,
        start: Instant,
        level: u8,
        duration: Duration,
    ) -> (InflationCommand, Instant) {
        let step = Duration::from_millis(100);
        let mut now = start;
        let end = start + duration;
        let mut last = controller.command(now);
        while now < end {
            now += step;
            controller.observe(now, level);
            last = controller.command(now);
        }
        (last, now)
    }

    fn duty(command: InflationCommand) -> u8 {
        match command.actuation {
            Actuation::Cycle(duty) => duty,
            Actuation::Vent => panic!("expected a cycling command, got a vent"),
        }
    }

    /// Duty on the first tick at a steady fatigue level.
    fn duty_at(level: u8) -> u8 {
        let start = Instant::now();
        let mut controller = InflationController::default();
        controller.arm_canopy(true);
        controller.observe(start, level);
        duty(controller.command(start + Duration::from_millis(100)))
    }

    #[test]
    fn rest_vents_and_stays_idle() {
        let mut controller = InflationController::default();
        let (command, _) = run(&mut controller, Instant::now(), 10, Duration::from_secs(2));
        assert_eq!(command.actuation, Actuation::Vent);
        assert_eq!(command.mode, InflationMode::Idle);
        assert_eq!(controller.charge(), 0.0);
    }

    #[test]
    fn every_inflating_command_sits_above_neutral() {
        let params = InflationParams::default();
        let start = Instant::now();
        let mut controller = InflationController::default();
        controller.arm_canopy(true);

        let mut now = start;
        for level in (0..=100).step_by(5) {
            for _ in 0..20 {
                controller.observe(now, level);
                let command = controller.command(now);
                if let Actuation::Cycle(duty) = command.actuation {
                    assert!(
                        duty <= params.duty_ceiling,
                        "duty {duty} exceeds the ceiling at F={level}"
                    );
                    // Only a breath may ever ask to exhale.
                    if duty < params.neutral_duty {
                        assert_eq!(command.mode, InflationMode::Cradle);
                    }
                }
                now += Duration::from_millis(100);
            }
        }
    }

    #[test]
    fn nudge_is_slow_and_rises_with_fatigue() {
        let params = InflationParams::default();
        let low = duty_at(35);
        let high = duty_at(55);

        assert!(low < high, "{low} !< {high}");
        // A nudge is a tap: barely off neutral, nowhere near the cradle's duty.
        assert!(low > params.neutral_duty);
        assert!(
            high < params.neutral_duty + (params.duty_ceiling - params.neutral_duty) / 4,
            "nudge duty {high} is not a micro-rise"
        );
    }

    #[test]
    fn cradle_speed_is_an_inverted_u() {
        let edge_low = duty_at(60);
        let peak = duty_at(72);
        let edge_high = duty_at(84);

        assert!(peak > edge_low, "peak {peak} !> low edge {edge_low}");
        assert!(peak > edge_high, "peak {peak} !> high edge {edge_high}");
        // The cradle is the one mode that runs at the loop's fastest duty — it
        // has 3–5 s to catch someone's arms.
        assert_eq!(peak, InflationParams::default().duty_ceiling);
    }

    #[test]
    fn canopy_needs_an_explicit_trigger() {
        let start = Instant::now();

        let mut unarmed = InflationController::default();
        let (command, _) = run(&mut unarmed, start, 95, Duration::from_secs(1));
        assert_eq!(
            command.mode,
            InflationMode::Cradle,
            "fatigue alone must not deploy a canopy"
        );

        let mut armed = InflationController::default();
        armed.arm_canopy(true);
        let (command, _) = run(&mut armed, start, 95, Duration::from_secs(1));
        assert_eq!(command.mode, InflationMode::Canopy);
    }

    #[test]
    fn canopy_speed_is_constant_in_fatigue() {
        assert_eq!(duty_at(85), duty_at(100));
    }

    #[test]
    fn disarming_canopy_demotes_without_venting() {
        let start = Instant::now();
        let mut controller = InflationController::default();
        controller.arm_canopy(true);
        let (_, now) = run(&mut controller, start, 95, Duration::from_secs(1));

        controller.arm_canopy(false);
        let command = controller.command(now + Duration::from_millis(100));
        assert_eq!(command.mode, InflationMode::Cradle);
        assert_ne!(command.actuation, Actuation::Vent);
    }

    #[test]
    fn a_transient_dip_does_not_release() {
        let start = Instant::now();
        let mut controller = InflationController::default();
        let (_, now) = run(&mut controller, start, 70, Duration::from_secs(1));
        assert_eq!(controller.mode(), InflationMode::Cradle);

        // A brief look-up: below the release threshold, but nowhere near the
        // dwell.
        let (command, now) = run(&mut controller, now, 20, Duration::from_secs(2));
        assert_ne!(command.actuation, Actuation::Vent);
        assert_eq!(controller.mode(), InflationMode::Cradle);

        // Back to work; the dwell must have been cleared, not merely paused.
        let (_, now) = run(&mut controller, now, 70, Duration::from_secs(1));
        let (command, _) = run(&mut controller, now, 20, Duration::from_secs(4));
        assert_ne!(command.actuation, Actuation::Vent);
    }

    #[test]
    fn a_sustained_drop_releases_after_the_dwell() {
        let start = Instant::now();
        let mut controller = InflationController::default();
        let (_, now) = run(&mut controller, start, 70, Duration::from_secs(1));

        let (command, _) = run(&mut controller, now, 20, Duration::from_secs(7));
        assert_eq!(command.actuation, Actuation::Vent);
        assert_eq!(controller.mode(), InflationMode::Idle);
        assert_eq!(controller.charge(), 0.0, "a release ends the episode");
    }

    #[test]
    fn hysteresis_holds_between_the_on_and_off_thresholds() {
        let start = Instant::now();
        let mut controller = InflationController::default();
        let (_, now) = run(&mut controller, start, 65, Duration::from_secs(1));
        assert_eq!(controller.mode(), InflationMode::Cradle);

        // 62 is under the cradle's entry threshold but inside the margin, so it
        // is not a release however long it lasts.
        let (command, _) = run(&mut controller, now, 62, Duration::from_secs(8));
        assert_eq!(controller.mode(), InflationMode::Cradle);
        assert_ne!(command.actuation, Actuation::Vent);
    }

    #[test]
    fn the_nudge_settles_at_neutral_once_its_budget_is_spent() {
        let params = InflationParams::default();
        let start = Instant::now();
        let mut controller = InflationController::default();

        let (command, _) = run(
            &mut controller,
            start,
            45,
            params.nudge_budget + Duration::from_secs(2),
        );
        assert_eq!(command.actuation, Actuation::Cycle(params.neutral_duty));
        assert_eq!(command.mode, InflationMode::Nudge);
    }

    #[test]
    fn the_cradle_breathes_once_it_is_formed() {
        let params = InflationParams::default();
        let start = Instant::now();
        let mut controller = InflationController::default();
        let (_, mut now) = run(
            &mut controller,
            start,
            70,
            params.cradle_budget + Duration::from_millis(500),
        );

        // Walk two breath cycles: the steady state must swing both ways about
        // neutral, and stay a micro-amplitude thing.
        let (mut lowest, mut highest) = (u8::MAX, u8::MIN);
        let end = now + params.breath_period * 2;
        while now < end {
            now += Duration::from_millis(100);
            controller.observe(now, 70);
            let duty = duty(controller.command(now));
            lowest = lowest.min(duty);
            highest = highest.max(duty);
        }

        assert!(lowest < params.neutral_duty, "the breath never exhaled");
        assert!(highest > params.neutral_duty, "the breath never inhaled");
        let swing = u32::from(highest - lowest);
        assert!(swing < 15, "breath swing of {swing} points is not micro");
        assert_eq!(controller.mode(), InflationMode::Cradle);
    }

    #[test]
    fn breathing_does_not_drift_into_the_ceiling() {
        let start = Instant::now();
        let mut controller = InflationController::default();

        // Form the cradle, then breathe for five minutes.
        let (_, now) = run(&mut controller, start, 70, Duration::from_secs(6));
        let formed = controller.charge();
        let (_, _) = run(&mut controller, now, 70, Duration::from_secs(300));

        let drift = (controller.charge() - formed).abs();
        assert!(
            drift < 0.5,
            "the steady state drifted by {drift} duty-seconds over five minutes"
        );
    }

    #[test]
    fn oscillating_fatigue_cannot_ratchet_the_sections() {
        let params = InflationParams::default();
        let start = Instant::now();
        let mut controller = InflationController::default();

        // Form the cradle, then bounce fatigue across its threshold. Each
        // re-entry must be free: the air for that mode was already delivered, so
        // nothing after this point may drive harder than a breath.
        let (_, mut now) = run(&mut controller, start, 70, Duration::from_secs(6));
        let breath_ceiling =
            params.neutral_duty + (params.breath_amplitude * 100.0).ceil() as u8 + 1;

        for _ in 0..8 {
            for level in [55, 70] {
                let end = now + Duration::from_secs(2);
                while now < end {
                    now += Duration::from_millis(100);
                    controller.observe(now, level);
                    let duty = duty(controller.command(now));
                    assert!(
                        duty <= breath_ceiling,
                        "re-entering the cradle re-inflated at duty {duty}"
                    );
                }
            }
        }
        assert!(controller.charge() <= params.max_charge);
    }

    #[test]
    fn the_charge_ceiling_bounds_a_stuck_alert() {
        let params = InflationParams::default();
        let start = Instant::now();
        let mut controller = InflationController::default();
        controller.arm_canopy(true);

        // Pinned at maximum fatigue for five minutes: every budget, then hold.
        // The ceiling is most of what stands between this and a burst section.
        let (command, _) = run(&mut controller, start, 100, Duration::from_secs(300));
        assert!(
            controller.charge() <= params.max_charge,
            "charge {} exceeded the ceiling",
            controller.charge()
        );
        assert_eq!(
            command.actuation,
            Actuation::Cycle(params.neutral_duty),
            "still inflating after five minutes at F=100"
        );
    }

    #[test]
    fn a_stale_verdict_holds_then_vents() {
        let params = InflationParams::default();
        let start = Instant::now();
        let mut controller = InflationController::default();
        let (_, now) = run(&mut controller, start, 70, Duration::from_secs(1));

        // Verdicts stop arriving (radar gap) while the cradle is still forming.
        let frozen = controller.command(now + params.verdict_timeout);
        assert_eq!(frozen.actuation, Actuation::Cycle(params.neutral_duty));

        let gone = controller.command(now + params.verdict_timeout * 2);
        assert_eq!(gone.actuation, Actuation::Vent);
        assert_eq!(controller.mode(), InflationMode::Idle);
    }

    #[test]
    fn a_rising_verdict_speeds_the_approach() {
        let start = Instant::now();
        let step = Duration::from_millis(100);

        // Steady at 65.
        let mut steady = InflationController::default();
        let mut now = start;
        for _ in 0..10 {
            steady.observe(now, 65);
            now += step;
        }
        let steady_duty = duty(steady.command(now));

        // Arriving at 65 fast, from below.
        let mut rising = InflationController::default();
        let mut now = start;
        for level in [40, 45, 50, 55, 60, 65] {
            rising.observe(now, level);
            now += step;
        }
        let rising_duty = duty(rising.command(now));

        assert!(
            rising_duty > steady_duty,
            "dF/dt term did not act: {rising_duty} !> {steady_duty}"
        );
    }
}
