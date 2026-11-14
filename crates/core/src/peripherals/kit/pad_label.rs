// SPDX-License-Identifier: MIT
//! Port-qualified labels must retain their concrete model's port identity.
//! Kept independent of the bus so boundary/refusal cases can run standalone.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PadFamily {
    Stm32,
    Sam,
    Avr,
}

impl PadFamily {
    pub(super) fn instance_name(self, port: u8) -> String {
        let prefix = match self {
            Self::Stm32 => "gpio",
            Self::Sam | Self::Avr => "port",
        };
        format!("{prefix}{}", char::from(b'a' + port))
    }
}

/// The caller admits only a named peripheral with the matching concrete model.
/// Unknown ports and invalid pads never fall through to ESP numeric parsing.
pub(super) fn parse_named_pad(
    label: &str,
    has_port: impl Fn(PadFamily, u8, u8) -> bool,
) -> Option<u8> {
    let rest = label.strip_prefix('P')?;
    let mut chars = rest.chars();
    let letter @ 'A'..='P' = chars.next()? else {
        return None;
    };
    let port = letter as u8 - b'A';
    let digits = chars.as_str();
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let pin: u8 = digits.parse().ok()?;
    // Refuse an ambiguous model assignment rather than silently selecting a
    // different port width. Real board ports use one concrete GPIO family.
    let width = match (
        has_port(PadFamily::Stm32, port, pin),
        has_port(PadFamily::Sam, port, pin),
        has_port(PadFamily::Avr, port, pin),
    ) {
        (true, false, false) => 16,
        (false, true, false) => 32,
        (false, false, true) => 8,
        _ => return None,
    };
    if pin >= width {
        return None;
    }
    port.checked_mul(width)?.checked_add(pin)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(label: &str, expected: PadFamily) -> Option<u8> {
        parse_named_pad(label, |family, port, _| family == expected && port < 2)
    }

    #[test]
    fn sam_pa15_and_32_pad_port_identity_are_preserved() {
        assert_eq!(parse("PA15", PadFamily::Sam), Some(15));
        assert_eq!(parse("PA31", PadFamily::Sam), Some(31));
        assert_eq!(parse("PB0", PadFamily::Sam), Some(32));
        assert_eq!(parse("PB31", PadFamily::Sam), Some(63));
        assert_eq!(PadFamily::Sam.instance_name(1), "portb");
    }

    #[test]
    fn stm32_pb0_is_not_esp_gpio0_or_sam_pb0() {
        assert_eq!(parse("PB0", PadFamily::Stm32), Some(16));
        assert_eq!(parse("PB15", PadFamily::Stm32), Some(31));
        assert_eq!(parse("PA16", PadFamily::Stm32), None);
        assert_eq!(PadFamily::Stm32.instance_name(1), "gpiob");
    }

    #[test]
    fn avr_pd6_retains_eight_pad_port_identity_and_boundaries() {
        let avr = |family, port, pin| family == PadFamily::Avr && port == 3 && pin < 8;
        assert_eq!(parse_named_pad("PD6", avr), Some(30));
        assert_eq!(parse_named_pad("PD7", avr), Some(31));
        for label in ["PD8", "PD16", "PC6", "PE6", "PD-1", "PD+1"] {
            assert_eq!(parse_named_pad(label, avr), None, "{label}");
        }
        assert_eq!(PadFamily::Avr.instance_name(3), "portd");
    }

    #[test]
    fn avr_reduced_bonding_and_cross_family_ambiguity_refuse() {
        let bonded = |family, port, pin| family == PadFamily::Avr && port == 0 && pin < 6;
        assert_eq!(parse_named_pad("PA5", bonded), Some(5));
        assert_eq!(parse_named_pad("PA6", bonded), None);
        assert_eq!(
            parse_named_pad("PB0", |family, _, _| family != PadFamily::Stm32),
            None
        );
        // The same label never aliases across concrete port families.
        assert_eq!(parse("PB0", PadFamily::Avr), Some(8));
        assert_eq!(parse("PB0", PadFamily::Stm32), Some(16));
        assert_eq!(parse("PB0", PadFamily::Sam), Some(32));
    }

    #[test]
    fn invalid_or_unmodeled_pads_are_not_numeric_fallbacks() {
        for label in [
            "PA32", "PB32", "PC0", "PA256", "P0", "PA", "PA-1", "PA+1", "PA 1", "GPIO15", "IO15",
            "15",
        ] {
            assert_eq!(parse(label, PadFamily::Sam), None, "{label}");
        }
        assert_eq!(parse_named_pad("PB0", |_, _, _| false), None);
        assert_eq!(parse_named_pad("PB0", |_, _, _| true), None);
        assert_eq!(parse_named_pad("PI0", |f, _, _| f == PadFamily::Sam), None);
    }
}
