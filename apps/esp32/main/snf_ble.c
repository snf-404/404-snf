#include "snf_ble.h"

#include <assert.h>
#include <math.h>
#include <string.h>

#include "esp_log.h"
#include "esp_random.h"
#include "esp_timer.h"
#include "freertos/FreeRTOS.h"
#include "freertos/task.h"
#include "host/ble_hs.h"
#include "host/util/util.h"
#include "nimble/nimble_port.h"
#include "nimble/nimble_port_freertos.h"
#include "os/os_mbuf.h"
#include "services/gap/ble_svc_gap.h"
#include "services/gatt/ble_svc_gatt.h"

#define SNF_PROTOCOL_MAJOR 1U
#define SNF_PROTOCOL_MINOR 0U
#define SNF_HEADER_LEN 16U

#define SNF_CAP_VITALS (1UL << 0)
#define SNF_CAP_ENCRYPTION_REQUIRED (1UL << 6)

#define SNF_STREAM_STATUS (1U << 0)
#define SNF_STREAM_VITALS (1U << 1)
#define SNF_SUPPORTED_STREAMS (SNF_STREAM_STATUS | SNF_STREAM_VITALS)

#define SNF_MESSAGE_STATUS 0x10U
#define SNF_MESSAGE_VITALS 0x20U
#define SNF_MESSAGE_CONTROL_RESPONSE 0x40U

#define SNF_FLAG_MORE_FRAGMENTS (1U << 0)
#define SNF_FLAG_SNAPSHOT (1U << 1)
#define SNF_FLAG_DEGRADED (1U << 2)
#define SNF_FLAG_STALE (1U << 3)

#define SNF_VITAL_SUBJECT_TRACKED (1U << 0)
#define SNF_VITAL_HEART_VALID (1U << 1)
#define SNF_VITAL_RESPIRATION_VALID (1U << 2)
#define SNF_VITAL_WARMING_UP (1U << 3)
#define SNF_VITAL_MOTION_CONTAMINATED (1U << 4)
#define SNF_VITAL_RADAR_GAP (1U << 6)

#define SNF_CONTROL_SET_STREAMS 0x01U
#define SNF_CONTROL_SET_SUBJECT 0x02U
#define SNF_CONTROL_REQUEST_SNAPSHOT 0x03U
#define SNF_CONTROL_PING 0x04U

#define SNF_CONTROL_SUCCESS 0U
#define SNF_CONTROL_UNSUPPORTED 1U
#define SNF_CONTROL_INVALID 2U
#define SNF_CONTROL_BUSY 3U

#define SNF_UNAVAILABLE_U16 UINT16_MAX
#define SNF_UNAVAILABLE_I16 INT16_MAX
#define SNF_CONTROL_MAX_ECHO 16U
#define SNF_CONTROL_RESPONSE_BASE_LEN 10U
#define SNF_CONTROL_RESPONSE_MAX_LEN (SNF_CONTROL_RESPONSE_BASE_LEN + SNF_CONTROL_MAX_ECHO)

static const char *TAG = "snf_ble";

/* UUID byte order is reversed for BLE_UUID128_INIT. */
static const ble_uuid128_t s_service_uuid =
    BLE_UUID128_INIT(0x00, 0x46, 0x4e, 0x53, 0x40, 0x40, 0x36, 0x9f,
                     0x2a, 0x4d, 0x44, 0x6b, 0x01, 0x00, 0x9f, 0x7b);
static const ble_uuid128_t s_protocol_info_uuid =
    BLE_UUID128_INIT(0x01, 0x46, 0x4e, 0x53, 0x40, 0x40, 0x36, 0x9f,
                     0x2a, 0x4d, 0x44, 0x6b, 0x01, 0x00, 0x9f, 0x7b);
static const ble_uuid128_t s_control_uuid =
    BLE_UUID128_INIT(0x02, 0x46, 0x4e, 0x53, 0x40, 0x40, 0x36, 0x9f,
                     0x2a, 0x4d, 0x44, 0x6b, 0x01, 0x00, 0x9f, 0x7b);
static const ble_uuid128_t s_status_uuid =
    BLE_UUID128_INIT(0x03, 0x46, 0x4e, 0x53, 0x40, 0x40, 0x36, 0x9f,
                     0x2a, 0x4d, 0x44, 0x6b, 0x01, 0x00, 0x9f, 0x7b);
static const ble_uuid128_t s_vitals_uuid =
    BLE_UUID128_INIT(0x04, 0x46, 0x4e, 0x53, 0x40, 0x40, 0x36, 0x9f,
                     0x2a, 0x4d, 0x44, 0x6b, 0x01, 0x00, 0x9f, 0x7b);

static uint16_t s_control_handle;
static uint16_t s_status_handle;
static uint16_t s_vitals_handle;
static uint16_t s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
static uint8_t s_own_addr_type;
static bool s_status_notify;
static bool s_vitals_notify;
static uint16_t s_active_streams = SNF_SUPPORTED_STREAMS;
static uint8_t s_vitals_hz = 2;
static uint32_t s_boot_id;
static uint32_t s_status_sequence;
static uint32_t s_vitals_sequence;
static uint32_t s_control_sequence;
static snf_ble_sample_t s_sample = {.warming_up = true};
static portMUX_TYPE s_sample_lock = portMUX_INITIALIZER_UNLOCKED;

typedef struct {
    bool active;
    bool in_flight;
    uint16_t request_id;
    uint8_t payload[SNF_CONTROL_RESPONSE_MAX_LEN];
    uint16_t payload_len;
    uint16_t offset;
    uint32_t sequence;
    uint32_t timestamp_ms;
} control_indication_t;

static control_indication_t s_control_indication;

typedef struct {
    bool active;
    uint16_t value_handle;
    uint8_t message_type;
    uint8_t flags;
    uint8_t payload[24];
    uint16_t payload_len;
    uint16_t offset;
    uint32_t sequence;
    uint32_t timestamp_ms;
} telemetry_notification_t;

static telemetry_notification_t s_status_notification;
static telemetry_notification_t s_vitals_notification;
static telemetry_notification_t *s_notification_in_flight;

static void put_u16(uint8_t *dst, uint16_t value)
{
    dst[0] = (uint8_t)value;
    dst[1] = (uint8_t)(value >> 8U);
}

static void put_i16(uint8_t *dst, int16_t value)
{
    put_u16(dst, (uint16_t)value);
}

static void put_u32(uint8_t *dst, uint32_t value)
{
    dst[0] = (uint8_t)value;
    dst[1] = (uint8_t)(value >> 8U);
    dst[2] = (uint8_t)(value >> 16U);
    dst[3] = (uint8_t)(value >> 24U);
}

static uint16_t get_u16(const uint8_t *src)
{
    return (uint16_t)src[0] | ((uint16_t)src[1] << 8U);
}

static uint16_t saturating_u16(uint32_t value)
{
    return value > UINT16_MAX ? UINT16_MAX : (uint16_t)value;
}

static uint8_t percent_from_float(float value)
{
    const float bounded = fminf(fmaxf(value, 0.0f), 1.0f);
    return (uint8_t)lroundf(bounded * 100.0f);
}

static uint8_t telemetry_flags(const snf_ble_sample_t *sample)
{
    uint8_t flags = 0;
    if (sample->motion_contaminated) {
        flags |= SNF_FLAG_DEGRADED;
        if (sample->respiration_bpm > 0.0f) {
            flags |= SNF_FLAG_STALE;
        }
    }
    return flags;
}

static void write_header(uint8_t *dst, uint8_t message_type, uint8_t flags,
                         uint32_t sequence, uint32_t timestamp_ms,
                         uint16_t payload_len,
                         uint16_t fragment_offset)
{
    dst[0] = SNF_PROTOCOL_MAJOR;
    dst[1] = message_type;
    dst[2] = flags;
    dst[3] = SNF_HEADER_LEN;
    put_u32(&dst[4], sequence);
    put_u32(&dst[8], timestamp_ms);
    put_u16(&dst[12], payload_len);
    put_u16(&dst[14], fragment_offset);
}

static void snapshot_sample(snf_ble_sample_t *sample)
{
    portENTER_CRITICAL(&s_sample_lock);
    *sample = s_sample;
    portEXIT_CRITICAL(&s_sample_lock);
}

static uint16_t make_status_payload(uint8_t payload[20])
{
    snf_ble_sample_t sample;
    snapshot_sample(&sample);
    memset(payload, 0, 20);
    put_u32(&payload[0], (uint32_t)(esp_timer_get_time() / 1000000));
    put_u16(&payload[4], s_active_streams);
    put_u16(&payload[6], 0);
    put_u16(&payload[8], 0);
    put_u16(&payload[10], 0);
    put_u16(&payload[12], saturating_u16(sample.ping_timeouts + sample.csi_drops));
    put_u16(&payload[14], SNF_UNAVAILABLE_U16);
    put_i16(&payload[16], SNF_UNAVAILABLE_I16);
    put_u16(&payload[18], 0);
    return 20;
}

static uint16_t make_vitals_payload(uint8_t payload[24], uint8_t *flags)
{
    snf_ble_sample_t sample;
    snapshot_sample(&sample);
    memset(payload, 0, 24);

    uint16_t status = 0;
    if (!sample.warming_up) {
        status |= SNF_VITAL_SUBJECT_TRACKED;
    }
    if (sample.respiration_valid) {
        status |= SNF_VITAL_RESPIRATION_VALID;
    }
    if (sample.warming_up) {
        status |= SNF_VITAL_WARMING_UP;
    }
    if (sample.motion_contaminated) {
        status |= SNF_VITAL_MOTION_CONTAMINATED;
    }
    if (sample.csi_drops > 0U) {
        status |= SNF_VITAL_RADAR_GAP;
    }

    put_u16(&payload[0], SNF_UNAVAILABLE_U16);
    put_u16(&payload[2], status);
    put_u16(&payload[4], SNF_UNAVAILABLE_U16);
    if ((sample.respiration_valid || sample.motion_contaminated) &&
        sample.respiration_bpm > 0.0f) {
        const uint32_t scaled = (uint32_t)lroundf(sample.respiration_bpm * 100.0f);
        put_u16(&payload[6], saturating_u16(scaled));
    } else {
        put_u16(&payload[6], SNF_UNAVAILABLE_U16);
    }
    payload[8] = 0;
    payload[9] = percent_from_float(sample.respiration_confidence);
    const float ratio = sample.motion_threshold > 0.0f
                            ? sample.motion_score / sample.motion_threshold
                            : 0.0f;
    payload[10] = percent_from_float(ratio);
    payload[11] = 0;
    put_u32(&payload[12], 0);
    put_u16(&payload[16], 0);
    put_u16(&payload[18], sample.motion ? 32767U : 0U);
    put_u16(&payload[20], SNF_UNAVAILABLE_U16);
    put_i16(&payload[22], 0);
    *flags = telemetry_flags(&sample);
    return 24;
}

static int append_message(struct os_mbuf *om, uint8_t message_type,
                          uint8_t flags, uint32_t sequence,
                          const uint8_t *payload, uint16_t payload_len)
{
    uint8_t header[SNF_HEADER_LEN];
    write_header(header, message_type, flags, sequence,
                 (uint32_t)(esp_timer_get_time() / 1000), payload_len, 0);
    int rc = os_mbuf_append(om, header, sizeof(header));
    if (rc == 0) {
        rc = os_mbuf_append(om, payload, payload_len);
    }
    return rc == 0 ? 0 : BLE_ATT_ERR_INSUFFICIENT_RES;
}

static bool connection_is_usable(void)
{
    if (s_conn_handle == BLE_HS_CONN_HANDLE_NONE) {
        return false;
    }
#ifdef CONFIG_CSI_BLE_REQUIRE_ENCRYPTION
    struct ble_gap_conn_desc desc;
    return ble_gap_conn_find(s_conn_handle, &desc) == 0 && desc.sec_state.encrypted;
#else
    return true;
#endif
}

static void send_next_notification(void)
{
    if (s_notification_in_flight != NULL || s_control_indication.active ||
        !connection_is_usable()) {
        return;
    }
    telemetry_notification_t *notification = s_vitals_notification.active
                                                     ? &s_vitals_notification
                                                     : s_status_notification.active
                                                           ? &s_status_notification
                                                           : NULL;
    if (notification == NULL) {
        return;
    }
    const uint16_t mtu = ble_att_mtu(s_conn_handle);
    const uint16_t att_payload = mtu > 3U ? mtu - 3U : 0U;
    if (att_payload <= SNF_HEADER_LEN) {
        notification->active = false;
        return;
    }
    const uint16_t fragment_capacity = att_payload - SNF_HEADER_LEN;
    const uint16_t remaining = notification->payload_len - notification->offset;
    const uint16_t fragment_len =
        remaining < fragment_capacity ? remaining : fragment_capacity;
    uint8_t message_flags = notification->flags;
    if ((uint16_t)(notification->offset + fragment_len) < notification->payload_len) {
        message_flags |= SNF_FLAG_MORE_FRAGMENTS;
    }
    uint8_t packet[SNF_HEADER_LEN + 24];
    write_header(packet, notification->message_type, message_flags,
                 notification->sequence, notification->timestamp_ms,
                 notification->payload_len, notification->offset);
    memcpy(&packet[SNF_HEADER_LEN],
           &notification->payload[notification->offset], fragment_len);
    struct os_mbuf *om =
        ble_hs_mbuf_from_flat(packet, SNF_HEADER_LEN + fragment_len);
    if (om == NULL) {
        notification->active = false;
        return;
    }
    /*
     * NimBLE can report BLE_GAP_EVENT_NOTIFY_TX synchronously from inside
     * ble_gatts_notify_custom().  Publish the pending state first so that the
     * callback can retire this fragment instead of missing the event and
     * leaving the telemetry queue permanently blocked.
     */
    const uint16_t previous_offset = notification->offset;
    notification->offset = (uint16_t)(notification->offset + fragment_len);
    s_notification_in_flight = notification;
    const int rc = ble_gatts_notify_custom(s_conn_handle,
                                           notification->value_handle, om);
    if (rc != 0) {
        if (s_notification_in_flight == notification) {
            s_notification_in_flight = NULL;
            notification->offset = previous_offset;
        }
        notification->active = false;
        ESP_LOGD(TAG, "notification failed: %d", rc);
        return;
    }
}

static void queue_notification(telemetry_notification_t *notification,
                               uint16_t value_handle, uint8_t message_type,
                               uint8_t flags, uint32_t sequence,
                               const uint8_t *payload, uint16_t payload_len)
{
    if (notification->active || payload_len > sizeof(notification->payload)) {
        return;
    }
    notification->active = true;
    notification->value_handle = value_handle;
    notification->message_type = message_type;
    notification->flags = flags;
    notification->payload_len = payload_len;
    notification->offset = 0;
    notification->sequence = sequence;
    notification->timestamp_ms = (uint32_t)(esp_timer_get_time() / 1000);
    memcpy(notification->payload, payload, payload_len);
    send_next_notification();
}

static void send_status(uint8_t flags)
{
    if (!s_status_notify || (s_active_streams & SNF_STREAM_STATUS) == 0U) {
        return;
    }
    uint8_t payload[20];
    const uint16_t len = make_status_payload(payload);
    queue_notification(&s_status_notification, s_status_handle,
                       SNF_MESSAGE_STATUS, flags, ++s_status_sequence,
                       payload, len);
}

static void send_vitals(uint8_t extra_flags)
{
    if (!s_vitals_notify || (s_active_streams & SNF_STREAM_VITALS) == 0U) {
        return;
    }
    uint8_t payload[24];
    uint8_t flags;
    const uint16_t len = make_vitals_payload(payload, &flags);
    queue_notification(&s_vitals_notification, s_vitals_handle,
                       SNF_MESSAGE_VITALS, flags | extra_flags,
                       ++s_vitals_sequence, payload, len);
}

static void send_next_control_fragment(void)
{
    if (!s_control_indication.active || s_control_indication.in_flight ||
        s_notification_in_flight != NULL ||
        !connection_is_usable()) {
        return;
    }
    const uint16_t mtu = ble_att_mtu(s_conn_handle);
    const uint16_t att_payload = mtu > 3U ? mtu - 3U : 0U;
    if (att_payload <= SNF_HEADER_LEN) {
        s_control_indication.active = false;
        return;
    }
    const uint16_t capacity = att_payload - SNF_HEADER_LEN;
    const uint16_t remaining = s_control_indication.payload_len - s_control_indication.offset;
    const uint16_t fragment_len = remaining < capacity ? remaining : capacity;
    uint8_t flags = 0;
    if ((uint16_t)(s_control_indication.offset + fragment_len) <
        s_control_indication.payload_len) {
        flags |= SNF_FLAG_MORE_FRAGMENTS;
    }
    uint8_t packet[SNF_HEADER_LEN + 244];
    write_header(packet, SNF_MESSAGE_CONTROL_RESPONSE, flags,
                 s_control_indication.sequence,
                 s_control_indication.timestamp_ms,
                 s_control_indication.payload_len,
                 s_control_indication.offset);
    memcpy(&packet[SNF_HEADER_LEN],
           &s_control_indication.payload[s_control_indication.offset], fragment_len);
    struct os_mbuf *om = ble_hs_mbuf_from_flat(packet, SNF_HEADER_LEN + fragment_len);
    if (om == NULL) {
        s_control_indication.active = false;
        return;
    }
    /* The indication completion event can also run before this call returns. */
    const uint16_t previous_offset = s_control_indication.offset;
    s_control_indication.offset =
        (uint16_t)(s_control_indication.offset + fragment_len);
    s_control_indication.in_flight = true;
    const int rc = ble_gatts_indicate_custom(s_conn_handle, s_control_handle, om);
    if (rc != 0) {
        if (s_control_indication.in_flight) {
            s_control_indication.in_flight = false;
            s_control_indication.offset = previous_offset;
        }
        s_control_indication.active = false;
        ESP_LOGD(TAG, "control indication failed: %d", rc);
        return;
    }
}

static void queue_control_response(uint16_t request_id, uint8_t opcode,
                                   uint8_t result, const uint8_t *echo,
                                   uint8_t echo_len)
{
    if (echo_len > SNF_CONTROL_MAX_ECHO) {
        echo_len = SNF_CONTROL_MAX_ECHO;
    }
    if (s_control_indication.active) {
        return;
    }
    memset(&s_control_indication, 0, sizeof(s_control_indication));
    s_control_indication.active = true;
    s_control_indication.request_id = request_id;
    s_control_indication.sequence = ++s_control_sequence;
    s_control_indication.timestamp_ms = (uint32_t)(esp_timer_get_time() / 1000);
    s_control_indication.payload_len = SNF_CONTROL_RESPONSE_BASE_LEN + echo_len;
    put_u16(&s_control_indication.payload[0], request_id);
    s_control_indication.payload[2] = opcode;
    s_control_indication.payload[3] = result;
    put_u16(&s_control_indication.payload[4], s_active_streams);
    s_control_indication.payload[6] = s_vitals_hz;
    s_control_indication.payload[7] = 0;
    s_control_indication.payload[8] = 0;
    s_control_indication.payload[9] = 0;
    if (echo_len > 0U) {
        memcpy(&s_control_indication.payload[SNF_CONTROL_RESPONSE_BASE_LEN], echo, echo_len);
    }
    send_next_control_fragment();
}

static int handle_control_write(struct ble_gatt_access_ctxt *ctxt)
{
    uint8_t request[32];
    uint16_t request_len = 0;
    if (ble_hs_mbuf_to_flat(ctxt->om, request, sizeof(request), &request_len) != 0) {
        return BLE_ATT_ERR_UNLIKELY;
    }
    if (request_len < 8U || request[0] != SNF_PROTOCOL_MAJOR ||
        get_u16(&request[6]) != 0U || get_u16(&request[4]) != request_len - 8U) {
        return BLE_ATT_ERR_INVALID_ATTR_VALUE_LEN;
    }
    const uint8_t opcode = request[1];
    const uint16_t request_id = get_u16(&request[2]);
    const uint8_t *payload = &request[8];
    const uint16_t payload_len = request_len - 8U;
    uint8_t result = SNF_CONTROL_SUCCESS;
    const uint8_t *echo = NULL;
    uint8_t echo_len = 0;

    if (s_control_indication.active) {
        queue_control_response(request_id, opcode, SNF_CONTROL_BUSY, NULL, 0);
        return 0;
    }

    switch (opcode) {
    case SNF_CONTROL_SET_STREAMS:
        if (payload_len != 8U || get_u16(&payload[6]) != 0U ||
            payload[2] < 1U || payload[2] > 10U) {
            result = SNF_CONTROL_INVALID;
            break;
        }
        s_active_streams = get_u16(payload) & SNF_SUPPORTED_STREAMS;
        s_vitals_hz = payload[2];
        if ((get_u16(payload) & ~SNF_SUPPORTED_STREAMS) != 0U) {
            result = SNF_CONTROL_UNSUPPORTED;
        }
        break;
    case SNF_CONTROL_SET_SUBJECT:
        if (payload_len != 2U) {
            result = SNF_CONTROL_INVALID;
        } else if (get_u16(payload) != SNF_UNAVAILABLE_U16) {
            result = SNF_CONTROL_UNSUPPORTED;
        }
        break;
    case SNF_CONTROL_REQUEST_SNAPSHOT:
        if (payload_len != 2U) {
            result = SNF_CONTROL_INVALID;
        } else {
            const uint16_t mask = get_u16(payload);
            if ((mask & SNF_STREAM_STATUS) != 0U) {
                send_status(SNF_FLAG_SNAPSHOT);
            }
            if ((mask & SNF_STREAM_VITALS) != 0U) {
                send_vitals(SNF_FLAG_SNAPSHOT);
            }
            if ((mask & ~SNF_SUPPORTED_STREAMS) != 0U) {
                result = SNF_CONTROL_UNSUPPORTED;
            }
        }
        break;
    case SNF_CONTROL_PING:
        if (payload_len > SNF_CONTROL_MAX_ECHO) {
            result = SNF_CONTROL_INVALID;
        } else {
            echo = payload;
            echo_len = (uint8_t)payload_len;
        }
        break;
    default:
        result = SNF_CONTROL_UNSUPPORTED;
        break;
    }
    queue_control_response(request_id, opcode, result, echo, echo_len);
    return 0;
}

static int characteristic_access(uint16_t conn_handle, uint16_t attr_handle,
                                 struct ble_gatt_access_ctxt *ctxt, void *arg)
{
    (void)conn_handle;
    (void)attr_handle;
    const ble_uuid_t *uuid = ctxt->chr->uuid;
    if (ble_uuid_cmp(uuid, &s_protocol_info_uuid.u) == 0) {
        uint8_t info[24] = {'S', 'N', 'F', '1', SNF_PROTOCOL_MAJOR,
                            SNF_PROTOCOL_MINOR, SNF_HEADER_LEN, 1};
        uint32_t capabilities = SNF_CAP_VITALS;
#ifdef CONFIG_CSI_BLE_REQUIRE_ENCRYPTION
        capabilities |= SNF_CAP_ENCRYPTION_REQUIRED;
#endif
        put_u32(&info[8], capabilities);
        put_u16(&info[12], 0);
        info[14] = 0;
        info[15] = 1;
        put_u32(&info[16], s_boot_id);
        put_u32(&info[20], 0);
        return os_mbuf_append(ctxt->om, info, sizeof(info)) == 0
                   ? 0
                   : BLE_ATT_ERR_INSUFFICIENT_RES;
    }
    if (ble_uuid_cmp(uuid, &s_status_uuid.u) == 0) {
        uint8_t payload[20];
        const uint16_t len = make_status_payload(payload);
        return append_message(ctxt->om, SNF_MESSAGE_STATUS, 0,
                              ++s_status_sequence, payload, len);
    }
    if (ble_uuid_cmp(uuid, &s_vitals_uuid.u) == 0) {
        uint8_t payload[24];
        uint8_t flags;
        const uint16_t len = make_vitals_payload(payload, &flags);
        return append_message(ctxt->om, SNF_MESSAGE_VITALS, flags,
                              ++s_vitals_sequence, payload, len);
    }
    if (ble_uuid_cmp(uuid, &s_control_uuid.u) == 0 &&
        ctxt->op == BLE_GATT_ACCESS_OP_WRITE_CHR) {
        return handle_control_write(ctxt);
    }
    return BLE_ATT_ERR_UNLIKELY;
}

static const struct ble_gatt_svc_def s_services[] = {
    {
        .type = BLE_GATT_SVC_TYPE_PRIMARY,
        .uuid = &s_service_uuid.u,
        .characteristics = (struct ble_gatt_chr_def[]){
            {.uuid = &s_protocol_info_uuid.u,
             .access_cb = characteristic_access,
             .flags = BLE_GATT_CHR_F_READ},
            {.uuid = &s_control_uuid.u,
             .access_cb = characteristic_access,
             .val_handle = &s_control_handle,
             .flags = BLE_GATT_CHR_F_WRITE | BLE_GATT_CHR_F_INDICATE},
            {.uuid = &s_status_uuid.u,
             .access_cb = characteristic_access,
             .val_handle = &s_status_handle,
             .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_NOTIFY},
            {.uuid = &s_vitals_uuid.u,
             .access_cb = characteristic_access,
             .val_handle = &s_vitals_handle,
             .flags = BLE_GATT_CHR_F_READ | BLE_GATT_CHR_F_NOTIFY},
            {0},
        },
    },
    {0},
};

static void advertise(void);

static int gap_event(struct ble_gap_event *event, void *arg)
{
    (void)arg;
    switch (event->type) {
    case BLE_GAP_EVENT_CONNECT:
        if (event->connect.status == 0) {
            s_conn_handle = event->connect.conn_handle;
            ESP_LOGI(TAG, "client connected");
#ifdef CONFIG_CSI_BLE_REQUIRE_ENCRYPTION
            ble_gap_security_initiate(s_conn_handle);
#endif
        } else {
            advertise();
        }
        return 0;
    case BLE_GAP_EVENT_DISCONNECT:
        s_conn_handle = BLE_HS_CONN_HANDLE_NONE;
        s_status_notify = false;
        s_vitals_notify = false;
        memset(&s_control_indication, 0, sizeof(s_control_indication));
        memset(&s_status_notification, 0, sizeof(s_status_notification));
        memset(&s_vitals_notification, 0, sizeof(s_vitals_notification));
        s_notification_in_flight = NULL;
        ESP_LOGI(TAG, "client disconnected; reason=%d", event->disconnect.reason);
        advertise();
        return 0;
    case BLE_GAP_EVENT_ADV_COMPLETE:
        advertise();
        return 0;
    case BLE_GAP_EVENT_SUBSCRIBE:
        if (event->subscribe.attr_handle == s_status_handle) {
            s_status_notify = event->subscribe.cur_notify != 0;
        } else if (event->subscribe.attr_handle == s_vitals_handle) {
            s_vitals_notify = event->subscribe.cur_notify != 0;
        }
        return 0;
    case BLE_GAP_EVENT_NOTIFY_TX:
        if (event->notify_tx.attr_handle == s_control_handle &&
            event->notify_tx.indication) {
            s_control_indication.in_flight = false;
            if (event->notify_tx.status != 0 && event->notify_tx.status != BLE_HS_EDONE) {
                s_control_indication.active = false;
            } else if (s_control_indication.offset >= s_control_indication.payload_len) {
                s_control_indication.active = false;
            } else {
                send_next_control_fragment();
            }
            if (!s_control_indication.active) {
                send_next_notification();
            }
        } else if (!event->notify_tx.indication &&
                   s_notification_in_flight != NULL &&
                   event->notify_tx.attr_handle ==
                       s_notification_in_flight->value_handle) {
            telemetry_notification_t *completed = s_notification_in_flight;
            s_notification_in_flight = NULL;
            if (event->notify_tx.status != 0 &&
                event->notify_tx.status != BLE_HS_EDONE) {
                completed->active = false;
            } else if (completed->offset >= completed->payload_len) {
                completed->active = false;
            }
            if (s_control_indication.active) {
                send_next_control_fragment();
            } else {
                send_next_notification();
            }
        }
        return 0;
    default:
        return 0;
    }
}

static void advertise(void)
{
    struct ble_hs_adv_fields fields;
    memset(&fields, 0, sizeof(fields));
    fields.flags = BLE_HS_ADV_F_DISC_GEN | BLE_HS_ADV_F_BREDR_UNSUP;
    fields.name = (uint8_t *)ble_svc_gap_device_name();
    fields.name_len = strlen((const char *)fields.name);
    fields.name_is_complete = 1;
    fields.uuids128 = (ble_uuid128_t *)&s_service_uuid;
    fields.num_uuids128 = 1;
    fields.uuids128_is_complete = 1;
    int rc = ble_gap_adv_set_fields(&fields);
    if (rc != 0) {
        ESP_LOGE(TAG, "cannot set advertising data: %d", rc);
        return;
    }
    struct ble_gap_adv_params params;
    memset(&params, 0, sizeof(params));
    params.conn_mode = BLE_GAP_CONN_MODE_UND;
    params.disc_mode = BLE_GAP_DISC_MODE_GEN;
    rc = ble_gap_adv_start(s_own_addr_type, NULL, BLE_HS_FOREVER,
                           &params, gap_event, NULL);
    if (rc != 0) {
        ESP_LOGE(TAG, "cannot start advertising: %d", rc);
    }
}

static void on_sync(void)
{
    int rc = ble_hs_util_ensure_addr(0);
    assert(rc == 0);
    rc = ble_hs_id_infer_auto(0, &s_own_addr_type);
    assert(rc == 0);
    advertise();
}

static void on_reset(int reason)
{
    ESP_LOGE(TAG, "host reset: %d", reason);
}

static void host_task(void *arg)
{
    (void)arg;
    nimble_port_run();
    nimble_port_freertos_deinit();
}

static void telemetry_task(void *arg)
{
    (void)arg;
    TickType_t last_status = 0;
    TickType_t last_vitals = 0;
    while (true) {
        const TickType_t now = xTaskGetTickCount();
        if (now - last_status >= pdMS_TO_TICKS(1000)) {
            last_status = now;
            send_status(0);
        }
        const uint8_t hz = s_vitals_hz == 0U ? 1U : s_vitals_hz;
        if (now - last_vitals >= pdMS_TO_TICKS(1000U / hz)) {
            last_vitals = now;
            send_vitals(0);
        }
        vTaskDelay(pdMS_TO_TICKS(20));
    }
}

void ble_store_config_init(void);

esp_err_t snf_ble_init(void)
{
    s_boot_id = esp_random();
    esp_err_t err = nimble_port_init();
    if (err != ESP_OK) {
        return err;
    }
    ble_hs_cfg.reset_cb = on_reset;
    ble_hs_cfg.sync_cb = on_sync;
#ifdef CONFIG_CSI_BLE_REQUIRE_ENCRYPTION
    ble_hs_cfg.sm_bonding = 1;
    ble_hs_cfg.sm_sc = 1;
    ble_hs_cfg.sm_our_key_dist = BLE_SM_PAIR_KEY_DIST_ENC;
    ble_hs_cfg.sm_their_key_dist = BLE_SM_PAIR_KEY_DIST_ENC;
#endif
    ble_svc_gap_init();
    ble_svc_gatt_init();
    int rc = ble_gatts_count_cfg(s_services);
    if (rc == 0) {
        rc = ble_gatts_add_svcs(s_services);
    }
    if (rc != 0) {
        return ESP_FAIL;
    }
    rc = ble_svc_gap_device_name_set("404-SNF");
    if (rc != 0) {
        return ESP_FAIL;
    }
    ble_store_config_init();
    nimble_port_freertos_init(host_task);
    if (xTaskCreate(telemetry_task, "snf_telemetry", 4096, NULL, 4, NULL) != pdPASS) {
        return ESP_ERR_NO_MEM;
    }
    ESP_LOGI(TAG, "BLE telemetry v1 ready");
    return ESP_OK;
}

void snf_ble_update(const snf_ble_sample_t *sample)
{
    if (sample == NULL) {
        return;
    }
    portENTER_CRITICAL(&s_sample_lock);
    s_sample = *sample;
    portEXIT_CRITICAL(&s_sample_lock);
}
