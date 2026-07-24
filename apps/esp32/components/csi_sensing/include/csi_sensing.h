#pragma once

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define CSI_SENSING_SUBCARRIERS 64
#define CSI_SENSING_SELECTED_SUBCARRIERS 12
#define CSI_SENSING_IQ_BYTES (CSI_SENSING_SUBCARRIERS * 2)

typedef enum {
    CSI_SENSING_CALIBRATING_SUBCARRIERS,
    CSI_SENSING_CALIBRATING_MOTION,
    CSI_SENSING_READY,
} csi_sensing_stage_t;

typedef struct {
    csi_sensing_stage_t stage;
    uint8_t calibration_percent;
    bool motion;
    float motion_score;
    float motion_threshold;
    bool breathing_suppressed;
    bool breathing_valid;
    float breathing_bpm;
    float breathing_confidence;
    int8_t rssi;
    uint32_t accepted_frames;
    uint32_t rejected_frames;
} csi_sensing_result_t;

typedef struct csi_sensing csi_sensing_t;

size_t csi_sensing_instance_size(void);
void csi_sensing_init(csi_sensing_t *sensing);

bool csi_sensing_push(csi_sensing_t *sensing,
                      const int8_t *iq,
                      size_t iq_len,
                      int64_t timestamp_us,
                      int8_t rssi,
                      csi_sensing_result_t *result);

const uint8_t *csi_sensing_selected_subcarriers(const csi_sensing_t *sensing);

#ifdef __cplusplus
}
#endif
