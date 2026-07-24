idf_component_register(
    SRCS "main.c" "snf_ble.c"
    INCLUDE_DIRS "."
    REQUIRES bt csi_sensing esp_event esp_netif esp_timer esp_wifi lwip nvs_flash
)
