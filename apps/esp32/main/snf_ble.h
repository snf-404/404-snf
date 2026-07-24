#pragma once

#include <stdbool.h>
#include <stdint.h>

#include "esp_err.h"

typedef struct {
    bool warming_up;
    bool motion;
    bool motion_contaminated;
    bool respiration_valid;
    float respiration_bpm;
    float respiration_confidence;
    float motion_score;
    float motion_threshold;
    uint32_t csi_drops;
    uint32_t ping_timeouts;
} snf_ble_sample_t;

esp_err_t snf_ble_init(void);
void snf_ble_update(const snf_ble_sample_t *sample);
