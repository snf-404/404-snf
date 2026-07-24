#include "csi_sensing.h"

#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>

#define PI_F 3.14159265358979323846f

static uint32_t s_random_state = 0x13579BDFU;

static float random_unit(void)
{
    s_random_state = (s_random_state * 1664525U) + 1013904223U;
    return ((float)((s_random_state >> 8U) & 0xFFFFU) / 32767.5f) - 1.0f;
}

static int8_t clamp_i8(float value)
{
    if (value > 127.0f) {
        value = 127.0f;
    } else if (value < -127.0f) {
        value = -127.0f;
    }
    return (int8_t)lroundf(value);
}

static void make_frame(int8_t *iq, float seconds, float breath_hz, bool motion)
{
    for (uint8_t carrier = 0; carrier < CSI_SENSING_SUBCARRIERS; ++carrier) {
        const float baseline = 32.0f + (0.35f * carrier);
        const float sensitivity = 0.015f + (0.002f * (carrier % 7U));
        float scale = 1.0f;
        if (breath_hz > 0.0f) {
            scale += sensitivity * sinf(2.0f * PI_F * breath_hz * seconds +
                                        0.03f * carrier);
        }
        if (motion) {
            scale += (0.15f + 0.01f * (carrier % 5U)) *
                     sinf((17.0f + carrier * 0.31f) * seconds + carrier);
            scale += random_unit() * 0.08f;
        } else {
            scale += random_unit() * 0.002f;
        }
        const float amplitude = baseline * scale;
        iq[(size_t)carrier * 2U] = clamp_i8(amplitude * 0.6f);
        iq[(size_t)carrier * 2U + 1U] = clamp_i8(amplitude * 0.8f);
    }
}

static csi_sensing_t *new_sensing(void)
{
    csi_sensing_t *sensing = calloc(1, csi_sensing_instance_size());
    assert(sensing != NULL);
    csi_sensing_init(sensing);
    return sensing;
}

static void calibrate(csi_sensing_t *sensing, csi_sensing_result_t *result)
{
    int8_t iq[CSI_SENSING_IQ_BYTES];
    for (uint32_t frame = 0; frame < 1000U; ++frame) {
        const float seconds = frame / 100.0f;
        make_frame(iq, seconds, 0.30f, false);
        assert(csi_sensing_push(sensing, iq, sizeof(iq), frame * 10000LL, -48,
                                result));
    }
    assert(result->stage == CSI_SENSING_READY);
    assert(result->calibration_percent == 100U);
    assert(result->motion_threshold > 0.0f);
}

static void test_rejects_short_frames(void)
{
    csi_sensing_t *sensing = new_sensing();
    csi_sensing_result_t result = {0};
    int8_t short_frame[16] = {0};
    assert(!csi_sensing_push(sensing, short_frame, sizeof(short_frame), 0, -70,
                             &result));
    assert(result.rejected_frames == 1U);
    free(sensing);
}

static void test_breathing_estimation(void)
{
    csi_sensing_t *sensing = new_sensing();
    csi_sensing_result_t result = {0};
    int8_t iq[CSI_SENSING_IQ_BYTES];
    calibrate(sensing, &result);

    for (uint32_t frame = 1000U; frame < 4200U; ++frame) {
        const float seconds = frame / 100.0f;
        make_frame(iq, seconds, 0.30f, false);
        csi_sensing_push(sensing, iq, sizeof(iq), frame * 10000LL, -49, &result);
    }

    assert(!result.motion);
    assert(result.breathing_valid);
    assert(fabsf(result.breathing_bpm - 18.0f) <= 1.0f);
    assert(result.breathing_confidence >= 0.5f);
    printf("breathing: %.2f bpm confidence=%.2f\n", result.breathing_bpm,
           result.breathing_confidence);
    free(sensing);
}

static void test_motion_state_machine(void)
{
    csi_sensing_t *sensing = new_sensing();
    csi_sensing_result_t result = {0};
    int8_t iq[CSI_SENSING_IQ_BYTES];
    calibrate(sensing, &result);

    for (uint32_t frame = 1000U; frame < 1300U; ++frame) {
        make_frame(iq, frame / 100.0f, 0.30f, false);
        csi_sensing_push(sensing, iq, sizeof(iq), frame * 10000LL, -50, &result);
    }
    assert(!result.motion);

    for (uint32_t frame = 1300U; frame < 1500U; ++frame) {
        make_frame(iq, frame / 100.0f, 0.0f, true);
        csi_sensing_push(sensing, iq, sizeof(iq), frame * 10000LL, -50, &result);
    }
    assert(result.motion);
    assert(!result.breathing_valid);
    printf("motion: score=%.8f threshold=%.8f\n", result.motion_score,
           result.motion_threshold);

    for (uint32_t frame = 1500U; frame < 1700U; ++frame) {
        make_frame(iq, frame / 100.0f, 0.30f, false);
        csi_sensing_push(sensing, iq, sizeof(iq), frame * 10000LL, -50, &result);
    }
    assert(!result.motion);
    free(sensing);
}

int main(void)
{
    test_rejects_short_frames();
    test_breathing_estimation();
    test_motion_state_machine();
    puts("all csi_sensing host tests passed");
    return 0;
}
