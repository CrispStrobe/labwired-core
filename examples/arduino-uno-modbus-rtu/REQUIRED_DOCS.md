# Required Source Documents (Uno + MAX485 + Modbus RTU)

1. Modbus Application Protocol Specification V1.1b3 (function codes 03, 04, 06,
   16, exception codes): https://modbus.org/docs/Modbus_Application_Protocol_V1_1b3.pdf
2. Modbus over Serial Line Specification and Implementation Guide V1.02 (RTU
   framing, the 3.5-character silence, CRC-16, 1.75 ms above 19200 baud):
   https://modbus.org/docs/Modbus_over_serial_line_V1_02.pdf
3. MAX485 datasheet, truth tables for the driver (DE, DI) and receiver (/RE, RO):
   https://www.analog.com/media/en/technical-documentation/data-sheets/MAX1487-MAX491.pdf
4. ModbusMaster library 2.0.1 (Apache-2.0): https://github.com/4-20ma/ModbusMaster
5. ATmega328P datasheet, USART0 (UCSR0A/B/C, UBRR0, UDR0, RX-complete vector):
   https://ww1.microchip.com/downloads/aemDocuments/documents/MCU08/ProductDocuments/DataSheets/ATmega48A-PA-88A-PA-168A-PA-328-P-DS-DS40002061B.pdf
6. pymodbus (BSD-3-Clause), the independent decoder in the gate:
   https://github.com/pymodbus-dev/pymodbus
