#include <inttypes.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#include "csi_sensing.h"
#include "esp_check.h"
#include "esp_event.h"
#include "esp_log.h"
#include "esp_netif.h"
#include "esp_timer.h"
#include "esp_wifi.h"
#include "freertos/FreeRTOS.h"
#include "freertos/event_groups.h"
#include "freertos/queue.h"
#include "freertos/task.h"
#include "lwip/ip_addr.h"
#include "nvs_flash.h"
#include "ping/ping_sock.h"
#include "sdkconfig.h"

#define WIFI_CONNECTED_BIT BIT0
#define WIFI_FAIL_BIT BIT1
#define CSI_QUEUE_DEPTH 32

static const char *TAG = "csi_app";
static EventGroupHandle_t s_wifi_events;
static esp_netif_t *s_sta_netif;
static QueueHandle_t s_csi_queue;
static int s_wifi_retries;
static uint32_t s_callback_drops;
static uint32_t s_ping_replies;
static uint32_t s_ping_timeouts;

typedef struct {
    int64_t timestamp_us;
    int8_t rssi;
    int8_t iq[CSI_SENSING_IQ_BYTES];
} csi_frame_t;

static void wifi_event_handler(void *arg,
                               esp_event_base_t event_base,
                               int32_t event_id,
                               void *event_data)
{
    (void)arg;
    (void)event_data;
    if (event_base == WIFI_EVENT && event_id == WIFI_EVENT_STA_DISCONNECTED) {
        if (s_wifi_retries++ < CONFIG_CSI_WIFI_MAXIMUM_RETRY) {
            esp_wifi_connect();
            ESP_LOGW(TAG, "Wi-Fi disconnected, retrying (%d/%d)", s_wifi_retries,
                     CONFIG_CSI_WIFI_MAXIMUM_RETRY);
        } else {
            xEventGroupSetBits(s_wifi_events, WIFI_FAIL_BIT);
        }
    } else if (event_base == IP_EVENT && event_id == IP_EVENT_STA_GOT_IP) {
        const ip_event_got_ip_t *event = (const ip_event_got_ip_t *)event_data;
        ESP_LOGI(TAG, "IPv4 address: " IPSTR ", gateway: " IPSTR,
                 IP2STR(&event->ip_info.ip), IP2STR(&event->ip_info.gw));
        s_wifi_retries = 0;
        xEventGroupSetBits(s_wifi_events, WIFI_CONNECTED_BIT);
    }
}

static esp_err_t wifi_connect(void)
{
    ESP_RETURN_ON_ERROR(esp_netif_init(), TAG, "esp_netif_init failed");
    ESP_RETURN_ON_ERROR(esp_event_loop_create_default(), TAG,
                        "esp_event_loop_create_default failed");
    s_sta_netif = esp_netif_create_default_wifi_sta();
    ESP_RETURN_ON_FALSE(s_sta_netif != NULL, ESP_ERR_NO_MEM, TAG,
                        "failed to create station network interface");

    wifi_init_config_t init = WIFI_INIT_CONFIG_DEFAULT();
    ESP_RETURN_ON_ERROR(esp_wifi_init(&init), TAG, "esp_wifi_init failed");

    s_wifi_events = xEventGroupCreate();
    ESP_RETURN_ON_FALSE(s_wifi_events != NULL, ESP_ERR_NO_MEM, TAG,
                        "failed to create Wi-Fi event group");
    ESP_RETURN_ON_ERROR(esp_event_handler_register(WIFI_EVENT, ESP_EVENT_ANY_ID,
                                                   wifi_event_handler, NULL),
                        TAG, "Wi-Fi handler registration failed");
    ESP_RETURN_ON_ERROR(esp_event_handler_register(IP_EVENT, IP_EVENT_STA_GOT_IP,
                                                   wifi_event_handler, NULL),
                        TAG, "IP handler registration failed");

    wifi_config_t config = {0};
    strlcpy((char *)config.sta.ssid, CONFIG_CSI_WIFI_SSID, sizeof(config.sta.ssid));
    strlcpy((char *)config.sta.password, CONFIG_CSI_WIFI_PASSWORD,
            sizeof(config.sta.password));
    config.sta.threshold.authmode = WIFI_AUTH_WPA2_PSK;
    config.sta.pmf_cfg.capable = true;
    config.sta.pmf_cfg.required = false;

    ESP_RETURN_ON_ERROR(esp_wifi_set_mode(WIFI_MODE_STA), TAG,
                        "esp_wifi_set_mode failed");
    ESP_RETURN_ON_ERROR(esp_wifi_set_config(WIFI_IF_STA, &config), TAG,
                        "esp_wifi_set_config failed");
    ESP_RETURN_ON_ERROR(esp_wifi_start(), TAG, "esp_wifi_start failed");
    ESP_RETURN_ON_ERROR(esp_wifi_set_band_mode(WIFI_BAND_MODE_2G_ONLY), TAG,
                        "failed to select the 2.4 GHz band");
    ESP_RETURN_ON_ERROR(esp_wifi_set_protocol(
                            WIFI_IF_STA,
                            WIFI_PROTOCOL_11B | WIFI_PROTOCOL_11G | WIFI_PROTOCOL_11N),
                        TAG, "failed to force 802.11n protocols");
    ESP_RETURN_ON_ERROR(esp_wifi_set_bandwidth(WIFI_IF_STA, WIFI_BW_HT20), TAG,
                        "failed to force HT20 bandwidth");
    ESP_RETURN_ON_ERROR(esp_wifi_set_ps(WIFI_PS_NONE), TAG,
                        "esp_wifi_set_ps failed");
    ESP_RETURN_ON_ERROR(esp_wifi_connect(), TAG, "esp_wifi_connect failed");

    const EventBits_t bits = xEventGroupWaitBits(
        s_wifi_events, WIFI_CONNECTED_BIT | WIFI_FAIL_BIT, pdFALSE, pdFALSE,
        portMAX_DELAY);
    return (bits & WIFI_CONNECTED_BIT) != 0U ? ESP_OK : ESP_FAIL;
}

static void csi_rx_callback(void *ctx, wifi_csi_info_t *data)
{
    (void)ctx;
    if (data == NULL || data->buf == NULL || data->len < CSI_SENSING_IQ_BYTES) {
        return;
    }

    csi_frame_t frame = {
        .timestamp_us = esp_timer_get_time(),
        .rssi = data->rx_ctrl.rssi,
    };
    memcpy(frame.iq, data->buf, sizeof(frame.iq));
    if (xQueueSend(s_csi_queue, &frame, 0) != pdTRUE) {
        ++s_callback_drops;
    }
}

static esp_err_t enable_csi(void)
{
    wifi_csi_config_t config = {
        .enable = 1,
        .acquire_csi_legacy = 1,
        .acquire_csi_ht20 = 1,
        .acquire_csi_ht40 = 0,
        .acquire_csi_su = 0,
        .acquire_csi_mu = 0,
        .acquire_csi_dcm = 0,
        .acquire_csi_beamformed = 0,
#if CONFIG_SOC_WIFI_MAC_VERSION_NUM == 3
        .acquire_csi_force_lltf = 0,
        .acquire_csi_vht = 0,
        .acquire_csi_he_stbc_mode = ESP_CSI_ACQUIRE_STBC_HELTF1,
#else
        .acquire_csi_he_stbc = ESP_CSI_ACQUIRE_STBC_HELTF1,
#endif
        .val_scale_cfg = 0,
        .dump_ack_en = 0,
    };
    ESP_RETURN_ON_ERROR(esp_wifi_set_csi_rx_cb(csi_rx_callback, NULL), TAG,
                        "esp_wifi_set_csi_rx_cb failed");
    ESP_RETURN_ON_ERROR(esp_wifi_set_csi_config(&config), TAG,
                        "esp_wifi_set_csi_config failed");
    ESP_RETURN_ON_ERROR(esp_wifi_set_csi(true), TAG, "esp_wifi_set_csi failed");
    return ESP_OK;
}

static void on_ping_success(esp_ping_handle_t handle, void *args)
{
    (void)handle;
    (void)args;
    ++s_ping_replies;
}

static void on_ping_timeout(esp_ping_handle_t handle, void *args)
{
    (void)handle;
    (void)args;
    ++s_ping_timeouts;
}

static esp_err_t start_gateway_ping(void)
{
    esp_netif_ip_info_t ip_info;
    ESP_RETURN_ON_ERROR(esp_netif_get_ip_info(s_sta_netif, &ip_info), TAG,
                        "failed to read station IP configuration");
    ESP_RETURN_ON_FALSE(ip_info.gw.addr != 0U, ESP_ERR_INVALID_STATE, TAG,
                        "DHCP did not provide a default gateway");

    ip_addr_t target_addr = IPADDR4_INIT(ip_info.gw.addr);
    esp_ping_config_t config = ESP_PING_DEFAULT_CONFIG();
    config.target_addr = target_addr;
    config.count = ESP_PING_COUNT_INFINITE;
    config.interval_ms = 1000U / CONFIG_CSI_PING_RATE_HZ;
    config.timeout_ms = 100;
    config.data_size = CONFIG_CSI_PING_DATA_SIZE;
    config.task_prio = 4;
    config.interface = (uint32_t)esp_netif_get_netif_impl_index(s_sta_netif);

    const esp_ping_callbacks_t callbacks = {
        .cb_args = NULL,
        .on_ping_success = on_ping_success,
        .on_ping_timeout = on_ping_timeout,
        .on_ping_end = NULL,
    };
    esp_ping_handle_t ping;
    ESP_RETURN_ON_ERROR(esp_ping_new_session(&config, &callbacks, &ping), TAG,
                        "esp_ping_new_session failed");
    ESP_RETURN_ON_ERROR(esp_ping_start(ping), TAG, "esp_ping_start failed");
    ESP_LOGI(TAG, "pinging gateway " IPSTR " at %d Hz with %d-byte payloads",
             IP2STR(&ip_info.gw), CONFIG_CSI_PING_RATE_HZ,
             CONFIG_CSI_PING_DATA_SIZE);
    return ESP_OK;
}

static void sensing_task(void *arg)
{
    (void)arg;
    csi_sensing_t *sensing = calloc(1, csi_sensing_instance_size());
    if (sensing == NULL) {
        ESP_LOGE(TAG, "cannot allocate sensing state (%u bytes)",
                 (unsigned)csi_sensing_instance_size());
        vTaskDelete(NULL);
        return;
    }
    csi_sensing_init(sensing);

    csi_frame_t frame;
    csi_sensing_result_t result = {0};
    int64_t next_log_us = 0;
    csi_sensing_stage_t last_stage = CSI_SENSING_CALIBRATING_SUBCARRIERS;
    bool last_motion = false;
    while (xQueueReceive(s_csi_queue, &frame, portMAX_DELAY) == pdTRUE) {
        csi_sensing_push(sensing, frame.iq, sizeof(frame.iq), frame.timestamp_us,
                         frame.rssi, &result);
        if (result.stage != last_stage) {
            last_stage = result.stage;
            if (result.stage == CSI_SENSING_READY) {
                const uint8_t *selected = csi_sensing_selected_subcarriers(sensing);
                ESP_LOGI(TAG,
                         "calibration complete; carriers=%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u,%u threshold=%.8f",
                         selected[0], selected[1], selected[2], selected[3],
                         selected[4], selected[5], selected[6], selected[7],
                         selected[8], selected[9], selected[10], selected[11],
                         result.motion_threshold);
            }
        }
        if (result.motion != last_motion) {
            last_motion = result.motion;
            ESP_LOGW(TAG, "motion=%s score=%.8f threshold=%.8f",
                     result.motion ? "true" : "false", result.motion_score,
                     result.motion_threshold);
        }
        if (frame.timestamp_us >= next_log_us) {
            next_log_us = frame.timestamp_us + 1000000LL;
            if (result.stage != CSI_SENSING_READY) {
                ESP_LOGI(TAG,
                         "calibrating=%u%% rssi=%d frames=%" PRIu32
                         " drops=%" PRIu32 " ping_replies=%" PRIu32
                         " ping_timeouts=%" PRIu32,
                         result.calibration_percent, result.rssi,
                         result.accepted_frames, s_callback_drops,
                         s_ping_replies, s_ping_timeouts);
            } else {
                ESP_LOGI(TAG,
                         "motion=%s score=%.8f breath=%s bpm=%.1f confidence=%.2f rssi=%d frames=%" PRIu32
                         " drops=%" PRIu32 " ping_replies=%" PRIu32
                         " ping_timeouts=%" PRIu32,
                         result.motion ? "true" : "false", result.motion_score,
                         result.breathing_valid ? "valid" : "waiting",
                         result.breathing_bpm, result.breathing_confidence,
                         result.rssi, result.accepted_frames, s_callback_drops,
                         s_ping_replies, s_ping_timeouts);
            }
        }
    }
}

void app_main(void)
{
    esp_err_t nvs_result = nvs_flash_init();
    if (nvs_result == ESP_ERR_NVS_NO_FREE_PAGES ||
        nvs_result == ESP_ERR_NVS_NEW_VERSION_FOUND) {
        ESP_ERROR_CHECK(nvs_flash_erase());
        nvs_result = nvs_flash_init();
    }
    ESP_ERROR_CHECK(nvs_result);
    ESP_ERROR_CHECK(wifi_connect());

    s_csi_queue = xQueueCreate(CSI_QUEUE_DEPTH, sizeof(csi_frame_t));
    ESP_ERROR_CHECK(s_csi_queue != NULL ? ESP_OK : ESP_ERR_NO_MEM);
    ESP_ERROR_CHECK(enable_csi());
    xTaskCreate(sensing_task, "csi_sensing", 6144, NULL, 5, NULL);
    ESP_ERROR_CHECK(start_gateway_ping());
    ESP_LOGI(TAG, "single-device sensor ready; keep the room still during calibration");
}
