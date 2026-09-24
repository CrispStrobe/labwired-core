// LabWired - Firmware Simulation Platform
// Copyright (C) 2026 Andrii Shylenko
// SPDX-License-Identifier: MIT

pub struct SwdDp {
    idcode: u32,
}

impl SwdDp {
    pub fn new(idcode: u32) -> Self {
        Self { idcode }
    }

    pub fn idcode(&self) -> u32 {
        self.idcode
    }
}
