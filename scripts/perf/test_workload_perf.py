from pathlib import Path

import pytest

import workload_perf as wp


def test_default_suite_covers_frame_event_and_identity_workloads():
    assert [case.name for case in wp.selected_cases(None)] == [
        "esp32c3-oled",
        "nrf54l15-embassy-events",
        "esp32c3-oled-identity",
    ]


def test_named_subset_keeps_declaration_order():
    selected = wp.selected_cases("esp32c3-oled-identity,esp32c3-oled")
    assert [case.name for case in selected] == [
        "esp32c3-oled",
        "esp32c3-oled-identity",
    ]


def test_unknown_workload_is_an_error():
    with pytest.raises(ValueError, match="unknown workload"):
        wp.selected_cases("not-a-workload")


def test_time_receipt_parser_preserves_rss_as_integer(tmp_path: Path):
    receipt = tmp_path / "time.txt"
    receipt.write_text(
        "wall_seconds=1.25\nuser_seconds=1.00\nsystem_seconds=0.20\nmax_rss_kib=4096\n"
    )
    assert wp.parse_time_receipt(receipt) == {
        "wall_seconds": 1.25,
        "user_seconds": 1.0,
        "system_seconds": 0.2,
        "max_rss_kib": 4096,
    }
