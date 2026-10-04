# External Components (Uno + MAX485 + Modbus RTU)

| Part | Where | Twin |
|------|-------|------|
| MAX485 | D1 to DI, D0 from RO, D2 to DE and /RE | `max485` kit. Byte-level gate on DE and /RE. |
| Modbus RTU sensor, address 1 | A/B pair | `modbus-rtu-sensor` data pack, 9600 baud |
| Modbus RTU sensor, address 2 | A/B pair | `modbus-rtu-sensor` data pack, 9600 baud |
| 120 Ohm resistor, two | Across A and B at each end of the pair | Diagram parts in the playground. The byte-level model does not use them. |

The USB bridge of a real Uno shares D0 and D1 with the MAX485. In the twin the
console shows what the firmware prints while DE is low; bytes the driver puts on
the pair are in the `rs485` bus log instead.
