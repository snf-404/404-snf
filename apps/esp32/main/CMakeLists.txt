idf_component_register(
    SRCS "main.c"
    INCLUDE_DIRS "."
    REQUIRES csi_sensing esp_event esp_netif esp_timer esp_wifi lwip nvs_flash
)
