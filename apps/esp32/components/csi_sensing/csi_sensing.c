#include "csi_sensing.h"

#include <math.h>
#include <stdlib.h>
#include <string.h>

#define SUBCARRIER_CALIBRATION_FRAMES 400
#define MOTION_CALIBRATION_FRAMES 600
#define MOTION_WINDOW_SIZE 100
#define MOTION_BASELINE_CAPACITY 500
#define MOTION_EVALUATION_STRIDE 10
#define HAMPEL_WINDOW_SIZE 7
#define BREATH_SAMPLE_RATE_HZ 10
#define BREATH_WINDOW_SIZE 256
#define BREATH_ANALYSIS_STRIDE 10
#define BREATH_CARRIERS 40
#define BREATH_BINS 105
#define BREATH_MIN_HZ 0.08f
#define BREATH_MAX_HZ 0.60f
#define BREATH_STEP_HZ 0.005f
#define PI_F 3.14159265358979323846f

typedef struct {
    double mean;
    double m2;
} running_stat_t;

struct csi_sensing {
    csi_sensing_stage_t stage;
    uint32_t stage_frames;
    uint32_t accepted_frames;
    uint32_t rejected_frames;
    int8_t rssi;

    running_stat_t carrier_stats[CSI_SENSING_SUBCARRIERS];
    uint8_t selected[CSI_SENSING_SELECTED_SUBCARRIERS];
    float carrier_baseline[CSI_SENSING_SELECTED_SUBCARRIERS];

    float hampel[HAMPEL_WINDOW_SIZE];
    uint8_t hampel_count;
    uint8_t hampel_pos;

    float motion_window[MOTION_WINDOW_SIZE];
    uint16_t motion_count;
    uint16_t motion_pos;
    double motion_sum;
    double motion_sum_sq;
    float baseline_scores[MOTION_BASELINE_CAPACITY];
    uint16_t baseline_score_count;
    float motion_score;
    float motion_threshold;
    float calibrated_threshold;
    uint8_t evaluation_counter;
    uint8_t motion_on_hits;
    uint8_t motion_off_hits;
    bool motion;
    int64_t motion_hold_until_us;

    uint8_t breath_carriers[BREATH_CARRIERS];
    float breath_accumulator[BREATH_CARRIERS];
    uint16_t breath_accumulator_count;
    int64_t next_breath_sample_us;
    float breath_samples[BREATH_CARRIERS][BREATH_WINDOW_SIZE];
    float hamming[BREATH_WINDOW_SIZE];
    uint16_t breath_count;
    uint16_t breath_pos;
    uint8_t breath_analysis_counter;
    bool breathing_valid;
    float breathing_bpm;
    float breathing_confidence;
};

static int compare_float(const void *lhs, const void *rhs)
{
    const float a = *(const float *)lhs;
    const float b = *(const float *)rhs;
    return (a > b) - (a < b);
}

static float magnitude_at(const int8_t *iq, uint8_t subcarrier)
{
    const float q = (float)iq[(size_t)subcarrier * 2U];
    const float i = (float)iq[(size_t)subcarrier * 2U + 1U];
    return sqrtf((i * i) + (q * q));
}

static void running_stat_push(running_stat_t *stat, double value, uint32_t count)
{
    const double delta = value - stat->mean;
    stat->mean += delta / (double)count;
    stat->m2 += delta * (value - stat->mean);
}

static bool is_usable_subcarrier(uint8_t index)
{
    return (index >= 6U && index <= 26U) || (index >= 38U && index <= 57U);
}

static void select_subcarriers(csi_sensing_t *sensing)
{
    float score[CSI_SENSING_SUBCARRIERS];
    bool used[CSI_SENSING_SUBCARRIERS] = {0};

    for (uint8_t i = 0; i < CSI_SENSING_SUBCARRIERS; ++i) {
        if (!is_usable_subcarrier(i) || sensing->carrier_stats[i].mean < 1.0) {
            score[i] = INFINITY;
            continue;
        }
        const double variance = sensing->carrier_stats[i].m2 /
                                (double)(SUBCARRIER_CALIBRATION_FRAMES - 1U);
        score[i] = (float)(sqrt(fmax(variance, 0.0)) / sensing->carrier_stats[i].mean);
    }

    for (uint8_t chosen = 0; chosen < CSI_SENSING_SELECTED_SUBCARRIERS; ++chosen) {
        uint8_t best = 0;
        float best_score = INFINITY;
        for (uint8_t i = 0; i < CSI_SENSING_SUBCARRIERS; ++i) {
            if (used[i] || score[i] >= best_score) {
                continue;
            }
            bool spaced = true;
            for (uint8_t j = 0; j < chosen; ++j) {
                const int distance = abs((int)i - (int)sensing->selected[j]);
                if (distance < 2) {
                    spaced = false;
                    break;
                }
            }
            if (spaced) {
                best = i;
                best_score = score[i];
            }
        }

        if (!isfinite(best_score)) {
            for (uint8_t i = 6U; i <= 57U; i += 4U) {
                if (is_usable_subcarrier(i) && !used[i]) {
                    best = i;
                    break;
                }
            }
        }
        sensing->selected[chosen] = best;
        sensing->carrier_baseline[chosen] = (float)fmax(sensing->carrier_stats[best].mean, 1.0);
        used[best] = true;
    }
}

static float median_of(float *values, size_t count)
{
    qsort(values, count, sizeof(values[0]), compare_float);
    if ((count & 1U) != 0U) {
        return values[count / 2U];
    }
    return 0.5f * (values[(count / 2U) - 1U] + values[count / 2U]);
}

static float hampel_filter(csi_sensing_t *sensing, float value)
{
    sensing->hampel[sensing->hampel_pos] = value;
    sensing->hampel_pos = (uint8_t)((sensing->hampel_pos + 1U) % HAMPEL_WINDOW_SIZE);
    if (sensing->hampel_count < HAMPEL_WINDOW_SIZE) {
        ++sensing->hampel_count;
        return value;
    }

    float sorted[HAMPEL_WINDOW_SIZE];
    float deviations[HAMPEL_WINDOW_SIZE];
    memcpy(sorted, sensing->hampel, sizeof(sorted));
    const float median = median_of(sorted, HAMPEL_WINDOW_SIZE);
    for (size_t i = 0; i < HAMPEL_WINDOW_SIZE; ++i) {
        deviations[i] = fabsf(sensing->hampel[i] - median);
    }
    const float mad = median_of(deviations, HAMPEL_WINDOW_SIZE);
    const float limit = 5.0f * 1.4826f * fmaxf(mad, 1.0e-6f);
    return fabsf(value - median) > limit ? median : value;
}

static float calculate_turbulence(const csi_sensing_t *sensing, const int8_t *iq)
{
    double mean = 0.0;
    double m2 = 0.0;
    for (uint8_t i = 0; i < CSI_SENSING_SELECTED_SUBCARRIERS; ++i) {
        const float amplitude = magnitude_at(iq, sensing->selected[i]);
        const double normalized = amplitude / sensing->carrier_baseline[i];
        const double delta = normalized - mean;
        mean += delta / (double)(i + 1U);
        m2 += delta * (normalized - mean);
    }
    const double variance = m2 / (double)(CSI_SENSING_SELECTED_SUBCARRIERS - 1U);
    return (float)(sqrt(fmax(variance, 0.0)) / fmax(mean, 1.0e-6));
}

static float push_motion_window(csi_sensing_t *sensing, float turbulence)
{
    if (sensing->motion_count == MOTION_WINDOW_SIZE) {
        const float removed = sensing->motion_window[sensing->motion_pos];
        sensing->motion_sum -= removed;
        sensing->motion_sum_sq -= (double)removed * removed;
    } else {
        ++sensing->motion_count;
    }

    sensing->motion_window[sensing->motion_pos] = turbulence;
    sensing->motion_pos = (uint16_t)((sensing->motion_pos + 1U) % MOTION_WINDOW_SIZE);
    sensing->motion_sum += turbulence;
    sensing->motion_sum_sq += (double)turbulence * turbulence;

    if (sensing->motion_count < MOTION_WINDOW_SIZE) {
        return 0.0f;
    }
    const double mean = sensing->motion_sum / MOTION_WINDOW_SIZE;
    const double variance = (sensing->motion_sum_sq / MOTION_WINDOW_SIZE) - (mean * mean);
    return (float)fmax(variance, 0.0);
}

static void finish_motion_calibration(csi_sensing_t *sensing)
{
    if (sensing->baseline_score_count == 0U) {
        sensing->motion_threshold = 1.0e-5f;
    } else {
        qsort(sensing->baseline_scores, sensing->baseline_score_count,
              sizeof(sensing->baseline_scores[0]), compare_float);
        const size_t p95_index = ((size_t)sensing->baseline_score_count * 95U) / 100U;
        const float p95 = sensing->baseline_scores[p95_index < sensing->baseline_score_count
                                                       ? p95_index
                                                       : sensing->baseline_score_count - 1U];
        sensing->motion_threshold = fmaxf(p95 * 1.75f, 1.0e-7f);
    }
    sensing->calibrated_threshold = sensing->motion_threshold;
    sensing->stage = CSI_SENSING_READY;
    sensing->stage_frames = 0;
}

static void update_motion_state(csi_sensing_t *sensing, int64_t timestamp_us)
{
    if (++sensing->evaluation_counter < MOTION_EVALUATION_STRIDE) {
        return;
    }
    sensing->evaluation_counter = 0;
    const bool hit = sensing->motion_score > sensing->motion_threshold;
    if (hit) {
        sensing->motion_off_hits = 0;
        if (sensing->motion_on_hits < UINT8_MAX) {
            ++sensing->motion_on_hits;
        }
        if (sensing->motion_on_hits >= 3U) {
            sensing->motion = true;
            sensing->motion_hold_until_us = timestamp_us + 3000000LL;
        }
    } else {
        sensing->motion_on_hits = 0;
        if (sensing->motion_off_hits < UINT8_MAX) {
            ++sensing->motion_off_hits;
        }
        if (sensing->motion_off_hits >= 3U) {
            sensing->motion = false;
        }
    }
}

static void analyze_breathing(csi_sensing_t *sensing, int64_t timestamp_us)
{
    if (sensing->motion || timestamp_us < sensing->motion_hold_until_us) {
        sensing->breathing_valid = false;
        return;
    }

    float combined_power[BREATH_BINS] = {0};
    float total_weight = 0.0f;
    uint16_t actual_bins = 0;
    for (uint8_t carrier = 0; carrier < BREATH_CARRIERS; ++carrier) {
        float mean = 0.0f;
        for (uint16_t n = 0; n < BREATH_WINDOW_SIZE; ++n) {
            const uint16_t index = (uint16_t)((sensing->breath_pos + n) % BREATH_WINDOW_SIZE);
            mean += sensing->breath_samples[carrier][index];
        }
        mean /= BREATH_WINDOW_SIZE;

        float carrier_best_power = 0.0f;
        float carrier_power[BREATH_BINS];
        float carrier_power_sum = 0.0f;
        uint16_t bins = 0;
        for (float hz = BREATH_MIN_HZ; hz <= BREATH_MAX_HZ + 0.0001f; hz += BREATH_STEP_HZ) {
            const float omega = 2.0f * PI_F * hz / BREATH_SAMPLE_RATE_HZ;
            const float cos_step = cosf(omega);
            const float sin_step = sinf(omega);
            float cos_n = 1.0f;
            float sin_n = 0.0f;
            float real = 0.0f;
            float imag = 0.0f;
            for (uint16_t n = 0; n < BREATH_WINDOW_SIZE; ++n) {
                const uint16_t index = (uint16_t)((sensing->breath_pos + n) % BREATH_WINDOW_SIZE);
                const float sample = (sensing->breath_samples[carrier][index] - mean) *
                                     sensing->hamming[n];
                real += sample * cos_n;
                imag -= sample * sin_n;
                const float next_cos = (cos_n * cos_step) - (sin_n * sin_step);
                sin_n = (sin_n * cos_step) + (cos_n * sin_step);
                cos_n = next_cos;
            }
            const float power = (real * real) + (imag * imag);
            carrier_power[bins] = power;
            carrier_power_sum += power;
            ++bins;
            if (power > carrier_best_power) {
                carrier_best_power = power;
            }
        }

        const float carrier_mean_power = carrier_power_sum / fmaxf((float)bins, 1.0f);
        const float prominence = carrier_best_power /
                                 fmaxf(carrier_mean_power, 1.0e-12f);
        const float weight = fminf(fmaxf(prominence - 1.0f, 0.1f), 12.0f);
        for (uint16_t bin = 0; bin < bins; ++bin) {
            combined_power[bin] += weight * carrier_power[bin];
        }
        total_weight += weight;
        actual_bins = bins;
    }

    float peak_power = 0.0f;
    float mean_power = 0.0f;
    uint16_t peak_bin = 0;
    for (uint16_t bin = 0; bin < actual_bins; ++bin) {
        combined_power[bin] /= fmaxf(total_weight, 1.0e-6f);
        mean_power += combined_power[bin];
        if (combined_power[bin] > peak_power) {
            peak_power = combined_power[bin];
            peak_bin = bin;
        }
    }
    mean_power /= fmaxf((float)actual_bins, 1.0f);
    const float peak_ratio = peak_power / fmaxf(mean_power, 1.0e-12f);
    sensing->breathing_bpm = (BREATH_MIN_HZ + peak_bin * BREATH_STEP_HZ) * 60.0f;
    sensing->breathing_confidence = fminf(fmaxf((peak_ratio - 2.0f) / 8.0f, 0.0f), 1.0f);
    sensing->breathing_valid = peak_ratio >= 6.0f;
}

static void update_breathing(csi_sensing_t *sensing,
                             const int8_t *iq,
                             int64_t timestamp_us)
{
    for (uint8_t i = 0; i < BREATH_CARRIERS; ++i) {
        const uint8_t carrier = sensing->breath_carriers[i];
        const float baseline = (float)fmax(sensing->carrier_stats[carrier].mean, 1.0);
        sensing->breath_accumulator[i] +=
            magnitude_at(iq, carrier) / baseline;
    }
    ++sensing->breath_accumulator_count;

    if (sensing->next_breath_sample_us == 0) {
        sensing->next_breath_sample_us = timestamp_us + 1000000LL / BREATH_SAMPLE_RATE_HZ;
        return;
    }
    if (timestamp_us < sensing->next_breath_sample_us) {
        return;
    }

    for (uint8_t i = 0; i < BREATH_CARRIERS; ++i) {
        sensing->breath_samples[i][sensing->breath_pos] =
            sensing->breath_accumulator[i] / sensing->breath_accumulator_count;
        sensing->breath_accumulator[i] = 0.0f;
    }
    sensing->breath_accumulator_count = 0;
    sensing->breath_pos = (uint16_t)((sensing->breath_pos + 1U) % BREATH_WINDOW_SIZE);
    if (sensing->breath_count < BREATH_WINDOW_SIZE) {
        ++sensing->breath_count;
    }
    do {
        sensing->next_breath_sample_us += 1000000LL / BREATH_SAMPLE_RATE_HZ;
    } while (sensing->next_breath_sample_us <= timestamp_us);

    if (sensing->breath_count == BREATH_WINDOW_SIZE &&
        ++sensing->breath_analysis_counter >= BREATH_ANALYSIS_STRIDE) {
        sensing->breath_analysis_counter = 0;
        analyze_breathing(sensing, timestamp_us);
    }
}

static void fill_result(const csi_sensing_t *sensing, csi_sensing_result_t *result)
{
    if (result == NULL) {
        return;
    }
    result->stage = sensing->stage;
    if (sensing->stage == CSI_SENSING_CALIBRATING_SUBCARRIERS) {
        result->calibration_percent = (uint8_t)((sensing->stage_frames * 40U) /
                                                SUBCARRIER_CALIBRATION_FRAMES);
    } else if (sensing->stage == CSI_SENSING_CALIBRATING_MOTION) {
        result->calibration_percent = (uint8_t)(40U +
            ((sensing->stage_frames * 60U) / MOTION_CALIBRATION_FRAMES));
    } else {
        result->calibration_percent = 100;
    }
    result->motion = sensing->motion;
    result->motion_score = sensing->motion_score;
    result->motion_threshold = sensing->motion_threshold;
    result->breathing_valid = sensing->breathing_valid;
    result->breathing_bpm = sensing->breathing_bpm;
    result->breathing_confidence = sensing->breathing_confidence;
    result->rssi = sensing->rssi;
    result->accepted_frames = sensing->accepted_frames;
    result->rejected_frames = sensing->rejected_frames;
}

size_t csi_sensing_instance_size(void)
{
    return sizeof(csi_sensing_t);
}

void csi_sensing_init(csi_sensing_t *sensing)
{
    memset(sensing, 0, sizeof(*sensing));
    sensing->stage = CSI_SENSING_CALIBRATING_SUBCARRIERS;
    uint8_t breath_index = 0;
    for (uint8_t carrier = 0;
         carrier < CSI_SENSING_SUBCARRIERS && breath_index < BREATH_CARRIERS;
         ++carrier) {
        if (is_usable_subcarrier(carrier)) {
            sensing->breath_carriers[breath_index++] = carrier;
        }
    }
    for (uint16_t n = 0; n < BREATH_WINDOW_SIZE; ++n) {
        sensing->hamming[n] = 0.54f - 0.46f *
            cosf((2.0f * PI_F * n) / (BREATH_WINDOW_SIZE - 1U));
    }
}

bool csi_sensing_push(csi_sensing_t *sensing,
                      const int8_t *iq,
                      size_t iq_len,
                      int64_t timestamp_us,
                      int8_t rssi,
                      csi_sensing_result_t *result)
{
    if (sensing == NULL || iq == NULL || iq_len < CSI_SENSING_IQ_BYTES) {
        if (sensing != NULL) {
            ++sensing->rejected_frames;
            fill_result(sensing, result);
        }
        return false;
    }

    ++sensing->accepted_frames;
    sensing->rssi = rssi;
    if (sensing->stage == CSI_SENSING_CALIBRATING_SUBCARRIERS) {
        ++sensing->stage_frames;
        for (uint8_t i = 0; i < CSI_SENSING_SUBCARRIERS; ++i) {
            running_stat_push(&sensing->carrier_stats[i], magnitude_at(iq, i),
                              sensing->stage_frames);
        }
        if (sensing->stage_frames >= SUBCARRIER_CALIBRATION_FRAMES) {
            select_subcarriers(sensing);
            sensing->stage = CSI_SENSING_CALIBRATING_MOTION;
            sensing->stage_frames = 0;
        }
        fill_result(sensing, result);
        return true;
    }

    const float turbulence = hampel_filter(sensing, calculate_turbulence(sensing, iq));
    sensing->motion_score = push_motion_window(sensing, turbulence);
    if (sensing->stage == CSI_SENSING_CALIBRATING_MOTION) {
        ++sensing->stage_frames;
        if (sensing->motion_count == MOTION_WINDOW_SIZE &&
            sensing->baseline_score_count < MOTION_BASELINE_CAPACITY) {
            sensing->baseline_scores[sensing->baseline_score_count++] = sensing->motion_score;
        }
        if (sensing->stage_frames >= MOTION_CALIBRATION_FRAMES) {
            finish_motion_calibration(sensing);
        }
    } else {
        update_motion_state(sensing, timestamp_us);
        update_breathing(sensing, iq, timestamp_us);
    }

    fill_result(sensing, result);
    return true;
}

const uint8_t *csi_sensing_selected_subcarriers(const csi_sensing_t *sensing)
{
    return sensing != NULL ? sensing->selected : NULL;
}
