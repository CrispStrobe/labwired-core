# Provenance of every interface constant

nrf-softdevice-hle is a clean-room, high-level emulation of the Nordic nRF51
SoftDevice API (S110 v8 family). **No SoftDevice, MBR or bootloader binary was
read, executed or disassembled** to write it, and no code from Nordic's nRF5
SDK or SoftDevice headers is reproduced. The emulator loads only the
application region of an image; the Nordic ranges stay unmapped/empty.

## Sources

| id | source | licence | used for |
|----|--------|---------|----------|
| H | `lancaster-university/nrf51-sdk` tag `v2.2.0+mb4`, `source/nordic_sdk/components/softdevice/s130/headers/*.h` (S130 v2 API, a superset of S110 v8's) — the SDK the micro:bit DAL builds against | "BSD-3clause-Nordic": plain BSD 3-clause, **no chip clause** (checked: no "integrated circuit" anywhere in the headers; `extract.sh` refuses if one appears) | SVC numbers, event ids, error codes, struct offsets. Values were **extracted mechanically**: enum/define evaluation (`facts/enums.py`) and `offsetof()` of a probe compiled with arm-none-eabi-gcc for Cortex-M0 (`facts/layout.c`, `facts/dumplayout.py`). The headers are fetched at extraction time, never committed. Tools: renode-spike-prime `tools/nrf-softdevice-hle/facts/`. |
| O | the official MakeCode images' own application regions (micro:bit DAL: MIT; ble-nrf51822/mbed: Apache-2.0; nrf51-sdk: BSD-3), running in the emulator | MIT / Apache-2.0 / BSD-3 | which calls are made, in which order, with which arguments; MBR-param reads; interrupt forwarding (VTOR = app base) |
| S | Bluetooth Core Specification v4.x/5.x | public spec | ATT (Vol 3 Part F), GATT (Part G), SMP legacy pairing c1/s1 (Part H 2.2.3/2.2.4) including the spec's sample values used as tests, L2CAP basic frames |
| R | nRF51 Series Reference Manual v3.0; nRF51822 PS | public docs | memory map, IRQ numbers (SWI2 = 22), flash page 1 KB, NVMC/UART/GPIO register offsets |
| F | FIPS-197 | public standard | AES-128 (`aes.rs`) with the Appendix B/C known answers |
| B | google/bumble 0.0.235 `bumble/link.py`, `bumble/ll.py` | Apache-2.0 | the shape of the shared air's BLE messages (LocalLink: advertising PDUs, LL control PDUs, ACL by address) |
| X | HLE-defined | — | behaviour the real SoftDevice leaves unspecified at API level (see table) |

Excluded, on purpose: nRF5 SDK 12+ / S112/S113/S132/S140 headers (Nordic
5-clause, chip clause 4 covers source), the Rust `nrf-softdevice` bindings
(generated from those headers), the SoftDevice/MBR hex images.

## SVC numbers [H]

Ranges: SDM 0x10 (`SDM_SVC_BASE`), MBR 0x18 (`MBR_SVC_BASE`), SoC 0x20
(`SOC_SVC_BASE`; S110 exposes 0x20-0x52), BLE common 0x60 (`BLE_SVC_BASE`),
GAP 0x70, GATTC 0x90, GATTS 0xA0, L2CAP 0xB0. Individual numbers are the
enum members in `src/facts.rs` `svc` (names as in the SDK so a reader can
check them), e.g. `sd_softdevice_is_enabled` 0x12, `sd_ble_evt_get` 0x61,
`sd_ble_gap_adv_start` 0x73, `sd_ble_gatts_hvx` 0xA6.

Measured [O]: the micro:bit V1 / Calliope mini application regions call only
numbers in these ranges (per-image lists in the README of the PR and in
`conformance/boot-*.json`).

## Event ids and error codes [H]

`BLE_EVT_TX_COMPLETE` 0x01, `BLE_GAP_EVT_*` 0x10.., `BLE_GATTS_EVT_*` 0x50..,
`NRF_EVT_*` 0..8, `NRF_ERROR_*` 0x0..0x11, SoC errors 0x2000+, BLE errors
0x3000+, `BLE_ERROR_GATTS_SYS_ATTR_MISSING` 0x3401 (= `NRF_GATTS_ERR_BASE`
0x3400 + 1). `SD_EVT_IRQn` = `SWI2_IRQn` (nrf_soc.h) = IRQ 22 [R].

## Struct layouts (Cortex-M0, AAPCS) [H, probe]

| struct | size | fields (offset) |
|--------|------|-----------------|
| ble_evt_t | 48 | header.evt_id 0, header.evt_len 2, evt 4 |
| ble_gap_evt_t | 42 | conn_handle 0, params 2 |
| ble_gap_addr_t | 7 | addr_type 0, addr 1 |
| ble_gap_evt_connected_t | 24 | peer_addr 0, own_addr 7, irk byte 14, conn_params 16 |
| ble_gap_conn_params_t | 8 | min 0, max 2, slave_latency 4, conn_sup_timeout 6 |
| ble_gap_evt_disconnected_t | 1 | reason 0 |
| ble_gatts_evt_t | 32 | conn_handle 0, params 2 |
| ble_gatts_evt_write_t | 28 | handle 0, op 2, context 4 (18), offset 22, len 24, data 26 |
| ble_gatts_attr_context_t | 18 | srvc_uuid 0, char_uuid 4, desc_uuid 8, srvc_handle 12, value_handle 14, type 16 |
| ble_common_evt_t | 16 | conn_handle 0, params 4 |
| ble_uuid_t | 4 | uuid 0, type 2 |
| ble_gatts_char_md_t | 28 | char_props 0, char_ext_props 1, p_char_user_desc 4, max 8, size 10, p_char_pf 12, p_user_desc_md 16, p_cccd_md 20, p_sccd_md 24 |
| ble_gatts_attr_t | 20 | p_uuid 0, p_attr_md 4, init_len 8, init_offs 10, max_len 12, p_value 16 |
| ble_gatts_attr_md_t | 3 | read_perm 0, write_perm 1, flags 2 (vlen:1 vloc:2 rd_auth:1 wr_auth:1) |
| ble_gatts_char_handles_t | 8 | value 0, user_desc 2, cccd 4, sccd 6 |
| ble_gatts_hvx_params_t | 16 | handle 0, type 2, offset 4, p_len 8, p_data 12 |
| ble_gatts_value_t | 8 | len 0, offset 2, p_value 4 |
| ble_gap_adv_params_t | 24 | type 0, p_peer_addr 4, fp 8, p_whitelist 12, interval 16, timeout 18 |
| ble_gap_sec_params_t | 5 | bond/mitm/io_caps/oob bits 0, min_key_size 1, max_key_size 2, kdist_periph 3, kdist_central 4 |
| ble_gap_evt_auth_status_t | 6 | auth_status 0, error_src/bonded 1, sm1_levels 2, sm2_levels 3, kdist_periph 4, kdist_central 5 |
| ble_version_t | 6 | version_number 0, company_id 2, subversion_number 4 |
| nrf_ecb_hal_data_t | 48 | key 0, cleartext 16, ciphertext 32 |

## HLE-defined behaviour [X]

| what | choice | why |
|------|--------|-----|
| reads of 0x0..app base | erased flash (0xFF) in Renode, zero in labwired (a plain memory region); never Nordic bytes | the app probes MBR words (bootloader address); both values mean "no bootloader" |
| ATT handles | GAP service 1-7 (device name, appearance, PPCP), GATT service 8, application from 9 | the S110 assigns its own handles; clients discover them |
| ATT_MTU | 23 | S110 v8 supports only the default MTU |
| device address | random static, from the backend config (C0:EE:...) | the SoftDevice derives it from FICR |
| advertising | one ADV message per interval (min 100 ms) on the air | no channel hopping on a virtual air |
| TX buffers | 6 | `sd_ble_tx_buffer_count_get` |
| sd_temp_get | 84 (21.00 degC) | fixed ambient |
| sd_ble_version_get | LL version 7 (Core 4.1), company 0x0059 (Nordic, Bluetooth SIG assigned numbers), subversion 0x0064 (the S110 v8.0.0 firmware id in Nordic's public nrfutil documentation) | the app only logs it |
| pairing | LE legacy Just Works (TK = 0), c1/s1 per spec; keys distributed as the app's keyset asks | the micro:bit default security level is "encryption, no MITM" |
| sd_nvic_critical_region_enter | disables the app's enabled IRQs except SWI2, re-enables them on exit | the real SD masks application interrupt priorities; the app cannot tell |
| SD event IRQ priority | `sd_nvic_EnableIRQ(SWI2)` raises SWI2 from priority 0 to 3 (NRF_APP_PRIORITY_LOW) if still 0 | the app never sets it, and an SVC from a priority-0 handler escalates to HardFault (measured in Renode: fault at the first sd_ble_evt_get after a connection) |
