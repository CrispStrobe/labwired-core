// SPDX-License-Identifier: MIT
//! Interface facts of the nRF51 SoftDevice API (S110 v8 / S130 v2 family) that
//! the HLE needs: supervisor-call numbers, event ids, error codes, struct
//! offsets. Numbers only — no Nordic code or header text is reproduced.
//!
//! Provenance (see PROVENANCE.md for the per-constant table):
//!   [H] lancaster-university/nrf51-sdk v2.2.0+mb4,
//!       source/nordic_sdk/components/softdevice/s130/headers/*.h —
//!       "BSD-3clause-Nordic" (plain BSD-3: no chip clause). The values were
//!       extracted mechanically (enum/define evaluation, and offsetof() of a
//!       probe compiled by arm-none-eabi-gcc for Cortex-M0); tools in
//!       tools/nrf-softdevice-hle/facts/ of renode-spike-prime.
//!   [O] observed: the supervisor calls and struct accesses of the official
//!       MakeCode micro:bit V1 / Calliope mini application regions (DAL MIT,
//!       BLE API Apache-2.0, nrf51-sdk BSD-3) running in the emulator.
//!   [S] Bluetooth Core Specification (ATT/L2CAP/SMP opcodes and layouts).
//!   [R] nRF51 Series Reference Manual (public): IRQ numbers, memory map.

/// SoftDevice Manager SVCs [H nrf_sdm.h, SDM_SVC_BASE 0x10].
pub mod svc {
    pub const SD_SOFTDEVICE_ENABLE: u8 = 0x10;
    pub const SD_SOFTDEVICE_DISABLE: u8 = 0x11;
    pub const SD_SOFTDEVICE_IS_ENABLED: u8 = 0x12;
    pub const SD_SOFTDEVICE_VECTOR_TABLE_BASE_SET: u8 = 0x13;
    /// MBR [H nrf_mbr.h, MBR_SVC_BASE 0x18].
    pub const SD_MBR_COMMAND: u8 = 0x18;
    // SoC library [H nrf_soc.h, SOC_SVC_BASE 0x20; 0x2B.. "not available" in S110].
    pub const SD_PPI_CHANNEL_ENABLE_GET: u8 = 0x20;
    pub const SD_PPI_CHANNEL_ENABLE_SET: u8 = 0x21;
    pub const SD_PPI_CHANNEL_ENABLE_CLR: u8 = 0x22;
    pub const SD_PPI_CHANNEL_ASSIGN: u8 = 0x23;
    pub const SD_PPI_GROUP_TASK_ENABLE: u8 = 0x24;
    pub const SD_PPI_GROUP_TASK_DISABLE: u8 = 0x25;
    pub const SD_PPI_GROUP_ASSIGN: u8 = 0x26;
    pub const SD_PPI_GROUP_GET: u8 = 0x27;
    pub const SD_FLASH_PAGE_ERASE: u8 = 0x28;
    pub const SD_FLASH_WRITE: u8 = 0x29;
    pub const SD_FLASH_PROTECT: u8 = 0x2A;
    pub const SD_MUTEX_NEW: u8 = 0x2B;
    pub const SD_MUTEX_ACQUIRE: u8 = 0x2C;
    pub const SD_MUTEX_RELEASE: u8 = 0x2D;
    pub const SD_NVIC_ENABLEIRQ: u8 = 0x2E;
    pub const SD_NVIC_DISABLEIRQ: u8 = 0x2F;
    pub const SD_NVIC_GETPENDINGIRQ: u8 = 0x30;
    pub const SD_NVIC_SETPENDINGIRQ: u8 = 0x31;
    pub const SD_NVIC_CLEARPENDINGIRQ: u8 = 0x32;
    pub const SD_NVIC_SETPRIORITY: u8 = 0x33;
    pub const SD_NVIC_GETPRIORITY: u8 = 0x34;
    pub const SD_NVIC_SYSTEMRESET: u8 = 0x35;
    pub const SD_NVIC_CRITICAL_REGION_ENTER: u8 = 0x36;
    pub const SD_NVIC_CRITICAL_REGION_EXIT: u8 = 0x37;
    pub const SD_RAND_APPLICATION_POOL_CAPACITY: u8 = 0x38;
    pub const SD_RAND_APPLICATION_BYTES_AVAILABLE: u8 = 0x39;
    pub const SD_RAND_APPLICATION_GET_VECTOR: u8 = 0x3A;
    pub const SD_POWER_MODE_SET: u8 = 0x3B;
    pub const SD_POWER_SYSTEM_OFF: u8 = 0x3C;
    pub const SD_POWER_RESET_REASON_GET: u8 = 0x3D;
    pub const SD_POWER_RESET_REASON_CLR: u8 = 0x3E;
    pub const SD_POWER_POF_ENABLE: u8 = 0x3F;
    pub const SD_POWER_POF_THRESHOLD_SET: u8 = 0x40;
    pub const SD_POWER_RAMON_SET: u8 = 0x41;
    pub const SD_POWER_RAMON_CLR: u8 = 0x42;
    pub const SD_POWER_RAMON_GET: u8 = 0x43;
    pub const SD_POWER_GPREGRET_SET: u8 = 0x44;
    pub const SD_POWER_GPREGRET_CLR: u8 = 0x45;
    pub const SD_POWER_GPREGRET_GET: u8 = 0x46;
    pub const SD_POWER_DCDC_MODE_SET: u8 = 0x47;
    pub const SD_APP_EVT_WAIT: u8 = 0x48;
    pub const SD_CLOCK_HFCLK_REQUEST: u8 = 0x49;
    pub const SD_CLOCK_HFCLK_RELEASE: u8 = 0x4A;
    pub const SD_CLOCK_HFCLK_IS_RUNNING: u8 = 0x4B;
    pub const SD_RADIO_NOTIFICATION_CFG_SET: u8 = 0x4C;
    pub const SD_ECB_BLOCK_ENCRYPT: u8 = 0x4D;
    pub const SD_RADIO_SESSION_OPEN: u8 = 0x4E;
    pub const SD_RADIO_SESSION_CLOSE: u8 = 0x4F;
    pub const SD_RADIO_REQUEST: u8 = 0x50;
    pub const SD_EVT_GET: u8 = 0x51;
    pub const SD_TEMP_GET: u8 = 0x52;
    // BLE common [H ble.h, BLE_SVC_BASE 0x60].
    pub const SD_BLE_ENABLE: u8 = 0x60;
    pub const SD_BLE_EVT_GET: u8 = 0x61;
    pub const SD_BLE_TX_BUFFER_COUNT_GET: u8 = 0x62;
    pub const SD_BLE_UUID_VS_ADD: u8 = 0x63;
    pub const SD_BLE_UUID_DECODE: u8 = 0x64;
    pub const SD_BLE_UUID_ENCODE: u8 = 0x65;
    pub const SD_BLE_VERSION_GET: u8 = 0x66;
    pub const SD_BLE_USER_MEM_REPLY: u8 = 0x67;
    pub const SD_BLE_OPT_SET: u8 = 0x68;
    pub const SD_BLE_OPT_GET: u8 = 0x69;
    // GAP [H ble_gap.h, BLE_GAP_SVC_BASE 0x70].
    pub const SD_BLE_GAP_ADDRESS_SET: u8 = 0x70;
    pub const SD_BLE_GAP_ADDRESS_GET: u8 = 0x71;
    pub const SD_BLE_GAP_ADV_DATA_SET: u8 = 0x72;
    pub const SD_BLE_GAP_ADV_START: u8 = 0x73;
    pub const SD_BLE_GAP_ADV_STOP: u8 = 0x74;
    pub const SD_BLE_GAP_CONN_PARAM_UPDATE: u8 = 0x75;
    pub const SD_BLE_GAP_DISCONNECT: u8 = 0x76;
    pub const SD_BLE_GAP_TX_POWER_SET: u8 = 0x77;
    pub const SD_BLE_GAP_APPEARANCE_SET: u8 = 0x78;
    pub const SD_BLE_GAP_APPEARANCE_GET: u8 = 0x79;
    pub const SD_BLE_GAP_PPCP_SET: u8 = 0x7A;
    pub const SD_BLE_GAP_PPCP_GET: u8 = 0x7B;
    pub const SD_BLE_GAP_DEVICE_NAME_SET: u8 = 0x7C;
    pub const SD_BLE_GAP_DEVICE_NAME_GET: u8 = 0x7D;
    pub const SD_BLE_GAP_AUTHENTICATE: u8 = 0x7E;
    pub const SD_BLE_GAP_SEC_PARAMS_REPLY: u8 = 0x7F;
    pub const SD_BLE_GAP_AUTH_KEY_REPLY: u8 = 0x80;
    pub const SD_BLE_GAP_ENCRYPT: u8 = 0x81;
    pub const SD_BLE_GAP_SEC_INFO_REPLY: u8 = 0x82;
    pub const SD_BLE_GAP_CONN_SEC_GET: u8 = 0x83;
    pub const SD_BLE_GAP_RSSI_START: u8 = 0x84;
    pub const SD_BLE_GAP_RSSI_STOP: u8 = 0x85;
    pub const SD_BLE_GAP_SCAN_START: u8 = 0x86;
    pub const SD_BLE_GAP_SCAN_STOP: u8 = 0x87;
    pub const SD_BLE_GAP_CONNECT: u8 = 0x88;
    pub const SD_BLE_GAP_CONNECT_CANCEL: u8 = 0x89;
    pub const SD_BLE_GAP_RSSI_GET: u8 = 0x8A;
    // GATT server [H ble_gatts.h, BLE_GATTS_SVC_BASE 0xA0].
    pub const SD_BLE_GATTS_SERVICE_ADD: u8 = 0xA0;
    pub const SD_BLE_GATTS_INCLUDE_ADD: u8 = 0xA1;
    pub const SD_BLE_GATTS_CHARACTERISTIC_ADD: u8 = 0xA2;
    pub const SD_BLE_GATTS_DESCRIPTOR_ADD: u8 = 0xA3;
    pub const SD_BLE_GATTS_VALUE_SET: u8 = 0xA4;
    pub const SD_BLE_GATTS_VALUE_GET: u8 = 0xA5;
    pub const SD_BLE_GATTS_HVX: u8 = 0xA6;
    pub const SD_BLE_GATTS_SERVICE_CHANGED: u8 = 0xA7;
    pub const SD_BLE_GATTS_RW_AUTHORIZE_REPLY: u8 = 0xA8;
    pub const SD_BLE_GATTS_SYS_ATTR_SET: u8 = 0xA9;
    pub const SD_BLE_GATTS_SYS_ATTR_GET: u8 = 0xAA;

    /// Name of a supervisor call, for traces. `None`: not in the family's table.
    pub fn name(n: u8) -> Option<&'static str> {
        Some(match n {
            0x10 => "sd_softdevice_enable",
            0x11 => "sd_softdevice_disable",
            0x12 => "sd_softdevice_is_enabled",
            0x13 => "sd_softdevice_vector_table_base_set",
            0x18 => "sd_mbr_command",
            0x20 => "sd_ppi_channel_enable_get",
            0x21 => "sd_ppi_channel_enable_set",
            0x22 => "sd_ppi_channel_enable_clr",
            0x23 => "sd_ppi_channel_assign",
            0x24 => "sd_ppi_group_task_enable",
            0x25 => "sd_ppi_group_task_disable",
            0x26 => "sd_ppi_group_assign",
            0x27 => "sd_ppi_group_get",
            0x28 => "sd_flash_page_erase",
            0x29 => "sd_flash_write",
            0x2A => "sd_flash_protect",
            0x2B => "sd_mutex_new",
            0x2C => "sd_mutex_acquire",
            0x2D => "sd_mutex_release",
            0x2E => "sd_nvic_EnableIRQ",
            0x2F => "sd_nvic_DisableIRQ",
            0x30 => "sd_nvic_GetPendingIRQ",
            0x31 => "sd_nvic_SetPendingIRQ",
            0x32 => "sd_nvic_ClearPendingIRQ",
            0x33 => "sd_nvic_SetPriority",
            0x34 => "sd_nvic_GetPriority",
            0x35 => "sd_nvic_SystemReset",
            0x36 => "sd_nvic_critical_region_enter",
            0x37 => "sd_nvic_critical_region_exit",
            0x38 => "sd_rand_application_pool_capacity_get",
            0x39 => "sd_rand_application_bytes_available_get",
            0x3A => "sd_rand_application_vector_get",
            0x3B => "sd_power_mode_set",
            0x3C => "sd_power_system_off",
            0x3D => "sd_power_reset_reason_get",
            0x3E => "sd_power_reset_reason_clr",
            0x3F => "sd_power_pof_enable",
            0x40 => "sd_power_pof_threshold_set",
            0x41 => "sd_power_ramon_set",
            0x42 => "sd_power_ramon_clr",
            0x43 => "sd_power_ramon_get",
            0x44 => "sd_power_gpregret_set",
            0x45 => "sd_power_gpregret_clr",
            0x46 => "sd_power_gpregret_get",
            0x47 => "sd_power_dcdc_mode_set",
            0x48 => "sd_app_evt_wait",
            0x49 => "sd_clock_hfclk_request",
            0x4A => "sd_clock_hfclk_release",
            0x4B => "sd_clock_hfclk_is_running",
            0x4C => "sd_radio_notification_cfg_set",
            0x4D => "sd_ecb_block_encrypt",
            0x4E => "sd_radio_session_open",
            0x4F => "sd_radio_session_close",
            0x50 => "sd_radio_request",
            0x51 => "sd_evt_get",
            0x52 => "sd_temp_get",
            0x60 => "sd_ble_enable",
            0x61 => "sd_ble_evt_get",
            0x62 => "sd_ble_tx_buffer_count_get",
            0x63 => "sd_ble_uuid_vs_add",
            0x64 => "sd_ble_uuid_decode",
            0x65 => "sd_ble_uuid_encode",
            0x66 => "sd_ble_version_get",
            0x67 => "sd_ble_user_mem_reply",
            0x68 => "sd_ble_opt_set",
            0x69 => "sd_ble_opt_get",
            0x70 => "sd_ble_gap_address_set",
            0x71 => "sd_ble_gap_address_get",
            0x72 => "sd_ble_gap_adv_data_set",
            0x73 => "sd_ble_gap_adv_start",
            0x74 => "sd_ble_gap_adv_stop",
            0x75 => "sd_ble_gap_conn_param_update",
            0x76 => "sd_ble_gap_disconnect",
            0x77 => "sd_ble_gap_tx_power_set",
            0x78 => "sd_ble_gap_appearance_set",
            0x79 => "sd_ble_gap_appearance_get",
            0x7A => "sd_ble_gap_ppcp_set",
            0x7B => "sd_ble_gap_ppcp_get",
            0x7C => "sd_ble_gap_device_name_set",
            0x7D => "sd_ble_gap_device_name_get",
            0x7E => "sd_ble_gap_authenticate",
            0x7F => "sd_ble_gap_sec_params_reply",
            0x80 => "sd_ble_gap_auth_key_reply",
            0x81 => "sd_ble_gap_encrypt",
            0x82 => "sd_ble_gap_sec_info_reply",
            0x83 => "sd_ble_gap_conn_sec_get",
            0x84 => "sd_ble_gap_rssi_start",
            0x85 => "sd_ble_gap_rssi_stop",
            0x86 => "sd_ble_gap_scan_start",
            0x87 => "sd_ble_gap_scan_stop",
            0x88 => "sd_ble_gap_connect",
            0x89 => "sd_ble_gap_connect_cancel",
            0x8A => "sd_ble_gap_rssi_get",
            0xA0 => "sd_ble_gatts_service_add",
            0xA1 => "sd_ble_gatts_include_add",
            0xA2 => "sd_ble_gatts_characteristic_add",
            0xA3 => "sd_ble_gatts_descriptor_add",
            0xA4 => "sd_ble_gatts_value_set",
            0xA5 => "sd_ble_gatts_value_get",
            0xA6 => "sd_ble_gatts_hvx",
            0xA7 => "sd_ble_gatts_service_changed",
            0xA8 => "sd_ble_gatts_rw_authorize_reply",
            0xA9 => "sd_ble_gatts_sys_attr_set",
            0xAA => "sd_ble_gatts_sys_attr_get",
            _ => return None,
        })
    }
}

/// Error codes [H nrf_error.h, nrf_error_sdm.h, nrf_error_soc.h, ble_err.h].
pub mod err {
    pub const NRF_SUCCESS: u32 = 0x0;
    pub const NRF_ERROR_SVC_HANDLER_MISSING: u32 = 0x1;
    pub const NRF_ERROR_SOFTDEVICE_NOT_ENABLED: u32 = 0x2;
    pub const NRF_ERROR_INTERNAL: u32 = 0x3;
    pub const NRF_ERROR_NO_MEM: u32 = 0x4;
    pub const NRF_ERROR_NOT_FOUND: u32 = 0x5;
    pub const NRF_ERROR_NOT_SUPPORTED: u32 = 0x6;
    pub const NRF_ERROR_INVALID_PARAM: u32 = 0x7;
    pub const NRF_ERROR_INVALID_STATE: u32 = 0x8;
    pub const NRF_ERROR_INVALID_LENGTH: u32 = 0x9;
    pub const NRF_ERROR_DATA_SIZE: u32 = 0xC;
    pub const NRF_ERROR_NULL: u32 = 0xE;
    pub const NRF_ERROR_INVALID_ADDR: u32 = 0x10;
    pub const NRF_ERROR_BUSY: u32 = 0x11;
    pub const NRF_ERROR_SOC_NVIC_INTERRUPT_NOT_AVAILABLE: u32 = 0x2001;
    pub const NRF_ERROR_SOC_RAND_NOT_ENOUGH_VALUES: u32 = 0x2007;
    pub const BLE_ERROR_NOT_ENABLED: u32 = 0x3001;
    pub const BLE_ERROR_INVALID_CONN_HANDLE: u32 = 0x3002;
    pub const BLE_ERROR_INVALID_ATTR_HANDLE: u32 = 0x3003;
    pub const BLE_ERROR_NO_TX_BUFFERS: u32 = 0x3004;
    pub const BLE_ERROR_GATTS_SYS_ATTR_MISSING: u32 = 0x3401;
}

/// Event ids [H ble.h, ble_gap.h, ble_gatts.h, nrf_soc.h].
pub mod evt {
    pub const BLE_EVT_TX_COMPLETE: u16 = 0x01;
    pub const BLE_GAP_EVT_CONNECTED: u16 = 0x10;
    pub const BLE_GAP_EVT_DISCONNECTED: u16 = 0x11;
    pub const BLE_GAP_EVT_CONN_PARAM_UPDATE: u16 = 0x12;
    pub const BLE_GAP_EVT_SEC_PARAMS_REQUEST: u16 = 0x13;
    pub const BLE_GAP_EVT_SEC_INFO_REQUEST: u16 = 0x14;
    pub const BLE_GAP_EVT_AUTH_STATUS: u16 = 0x17;
    pub const BLE_GAP_EVT_CONN_SEC_UPDATE: u16 = 0x18;
    pub const BLE_GAP_EVT_TIMEOUT: u16 = 0x19;
    pub const BLE_GATTS_EVT_WRITE: u16 = 0x50;
    pub const BLE_GATTS_EVT_RW_AUTHORIZE_REQUEST: u16 = 0x51;
    pub const BLE_GATTS_EVT_SYS_ATTR_MISSING: u16 = 0x52;
    pub const BLE_GATTS_EVT_HVC: u16 = 0x53;
    pub const NRF_EVT_HFCLKSTARTED: u32 = 0x0;
    pub const NRF_EVT_FLASH_OPERATION_SUCCESS: u32 = 0x2;
    pub const NRF_EVT_FLASH_OPERATION_ERROR: u32 = 0x3;
}

/// Misc constants [H].
pub mod k {
    pub const BLE_CONN_HANDLE_INVALID: u16 = 0xFFFF;
    pub const BLE_GAP_ROLE_PERIPH: u8 = 1;
    pub const BLE_GAP_ADDR_TYPE_PUBLIC: u8 = 0;
    pub const BLE_GAP_ADDR_TYPE_RANDOM_STATIC: u8 = 1;
    pub const BLE_GAP_ADV_TYPE_ADV_IND: u8 = 0;
    pub const BLE_GAP_ADV_TYPE_ADV_NONCONN_IND: u8 = 3;
    pub const BLE_UUID_TYPE_UNKNOWN: u8 = 0;
    pub const BLE_UUID_TYPE_BLE: u8 = 1;
    pub const BLE_UUID_TYPE_VENDOR_BEGIN: u8 = 2;
    pub const BLE_GATT_HVX_NOTIFICATION: u8 = 1;
    pub const BLE_GATT_HVX_INDICATION: u8 = 2;
    pub const BLE_GATTS_OP_WRITE_REQ: u8 = 1;
    pub const BLE_GATTS_OP_WRITE_CMD: u8 = 2;
    pub const BLE_GATTS_SRVC_TYPE_PRIMARY: u8 = 1;
    pub const BLE_HCI_REMOTE_USER_TERMINATED_CONNECTION: u8 = 0x13;
    pub const BLE_HCI_LOCAL_HOST_TERMINATED_CONNECTION: u8 = 0x16;
    /// SD_EVT_IRQn = SWI2_IRQn [H nrf_soc.h]; SWI2 is IRQ 22 on nRF51 [R].
    pub const SD_EVT_IRQN: u32 = 22;
}

/// Struct layouts for Cortex-M0 (arm-none-eabi-gcc, AAPCS) [H, offsetof probe].
pub mod layout {
    /// ble_evt_t: header {evt_id u16 @0, evt_len u16 @2}, evt union @4.
    pub const BLE_EVT_HDR: u32 = 4;
    /// ble_gap_evt_t: conn_handle u16 @0, params @2.
    pub const GAP_EVT_PARAMS: u32 = 2;
    /// ble_gap_evt_connected_t: peer_addr @0 (7), own_addr @7 (7),
    /// irk_match:1/irk_match_idx:7 byte @14, conn_params @16 (8). size 24.
    pub const CONNECTED_PEER_ADDR: u32 = 0;
    pub const CONNECTED_OWN_ADDR: u32 = 7;
    pub const CONNECTED_IRK: u32 = 14;
    pub const CONNECTED_PARAMS: u32 = 16;
    pub const CONNECTED_SIZE: u32 = 24;
    /// ble_gatts_evt_t: conn_handle u16 @0, params @2.
    pub const GATTS_EVT_PARAMS: u32 = 2;
    /// ble_gatts_evt_write_t: handle @0, op @2, context @4 (18), offset @22, len @24, data @26.
    pub const WRITE_HANDLE: u32 = 0;
    pub const WRITE_OP: u32 = 2;
    pub const WRITE_CONTEXT: u32 = 4;
    pub const WRITE_OFFSET: u32 = 22;
    pub const WRITE_LEN: u32 = 24;
    pub const WRITE_DATA: u32 = 26;
    /// ble_gatts_attr_context_t: srvc_uuid @0, char_uuid @4, desc_uuid @8,
    /// srvc_handle @12, value_handle @14, type @16.
    /// ble_common_evt_t: conn_handle @0, params @4; ble_evt_tx_complete_t.count @0.
    pub const COMMON_EVT_PARAMS: u32 = 4;
    /// ble_uuid_t: uuid u16 @0, type u8 @2 (size 4).
    /// ble_gatts_char_md_t: char_props @0, char_ext_props @1, p_char_user_desc @4,
    /// char_user_desc_max_size @8, char_user_desc_size @10, p_char_pf @12,
    /// p_user_desc_md @16, p_cccd_md @20, p_sccd_md @24.
    /// ble_gatts_attr_t: p_uuid @0, p_attr_md @4, init_len @8, init_offs @10,
    /// max_len @12, p_value @16.
    /// ble_gatts_attr_md_t: read_perm @0, write_perm @1, flags byte @2
    /// (vlen:1, vloc:2, rd_auth:1, wr_auth:1).
    /// ble_gatts_char_handles_t: value @0, user_desc @2, cccd @4, sccd @6.
    /// ble_gatts_hvx_params_t: handle @0, type @2, offset @4, p_len @8, p_data @12.
    /// ble_gatts_value_t: len @0, offset @2, p_value @4.
    /// ble_gap_adv_params_t: type @0, p_peer_addr @4, fp @8, p_whitelist @12,
    /// interval @16, timeout @18.
    pub const _DOC: () = ();
}
