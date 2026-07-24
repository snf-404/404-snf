# Device-tree fragment

[`snf-peripherals.dtsi`](snf-peripherals.dtsi) enables the two AP-side PWM
providers the pneumatics need. That is all it does.

`csti` emits the UIO carveouts, the IPCC doorbell node and the remoteproc
fix-ups into `dist/dts/linux/consortium-stm32mp257f-dk.dts`, and it lowers
`[peripheral.*]` bring-up for **I2C and GPIO blocks only**. Timers and PWM are
outside that, so the `[peripheral.tim4]` / `[peripheral.tim5]` entries in
`Consortium.toml` are declarative and this fragment is what makes them real.

| Node               | Change                                                       |
| ------------------ | ------------------------------------------------------------ |
| `&timers4` + `pwm` | enable; mux `TIM4_CH2` onto `PA1` (connector pin 33) — pump  |
| `&timers5` + `pwm` | enable; mux `TIM5_CH1` onto `PH8` (connector pin 31) — valve |

**Nothing here touches RIF, and nothing here touches USART6.** That port and its
pins (`PF13`/`PF14`) already belong to the CM33 on this board; the firmware just
uses them.

## ⚠️ Confirm the timer alternate-function numbers

`AF2` for `TIM4_CH2`/`PA1` and `TIM5_CH1`/`PH8` is a **placeholder that has not
been verified against the STM32MP25 reference manual**. The AF map is per-pin,
not per-timer, and MP25's is scattered — the vendor pinctrl `.dtsi` puts
`TIM3_CH2` at `AF7`, `TIM8_CH1/CH4` at `AF8` and `TIM10_CH1`/`TIM12_CH2` at
`AF9` — so the "TIM3/4/5 are AF2" rule from earlier STM32 parts does not carry
over.

Look up `PA1` and `PH8` in RM0457's alternate-function mapping table (or the
STM32MP257 datasheet pin tables) and fix the two `CHECK AF` lines. The symptom of
a wrong AF is a `pwmchip` that exports and accepts duty values while the pin
never moves.

## Applying it

The generated tree is a full merged `.dts` (`[dts] mode = "merge"`), so append
this fragment before compiling:

```bash
cat hardware/dts/snf-peripherals.dtsi >> dist/dts/linux/consortium-stm32mp257f-dk.dts
```

Then compile as `csti build` prints — it needs a kernel checkout for the
`dt-bindings` headers, which are GPL-2.0 and therefore never bundled:

```bash
cpp -nostdinc -I "$KERNEL/include" -undef -x assembler-with-cpp \
    dist/dts/linux/consortium-stm32mp257f-dk.dts | \
  dtc -I dts -O dtb -o stm32mp257f-dk.dtb -
```

Setting `[dts].linux` in `Consortium.toml` to that checkout makes the pipeline do
the preprocess-and-compile step itself.

## Checking it landed

```bash
for chip in /sys/class/pwm/pwmchip*; do echo "$chip -> $(readlink -f "$chip/device")"; done
```

`40020000.timer` is TIM4 (pump), `40030000.timer` is TIM5 (valve). On this board
they land at **`pwmchip4`** and **`pwmchip8`**, which is what `PneumaticConfig` in
[`crates/app/src/pneumatics.rs`](../../crates/app/src/pneumatics.rs) defaults to.
Those indices come from probe order and are **not** stable across kernel or
device-tree changes, so re-check after any change to either and update the
config.

Channels within a chip are numbered by the **timer's own** channel index, so the
pump is `pwmchip4/pwm1` (`TIM4_CH2`) and the valve is `pwmchip8/pwm0`
(`TIM5_CH1`).

A quick way to confirm the pin actually moves — and thus that the `AF` above is
right — without any hardware attached, is to drive one channel to 50 % and scope
or meter it:

```bash
cd /sys/class/pwm/pwmchip4 && echo 1 > export && cd pwm1 &&
  echo 1000000 > period && echo 500000 > duty_cycle && echo 1 > enable
```
