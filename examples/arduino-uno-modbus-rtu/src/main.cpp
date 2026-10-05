// Arduino Uno as a Modbus RTU master over a MAX485 transceiver.
//
//   D1 (TX) -> DI     D0 (RX) <- RO     D2 -> DE and /RE (tied together)
//
// Two XY-MD02 sensors share the A/B pair, at addresses 1 and 2. Each poll
// reads temperature and humidity (input registers 0x0001 and 0x0002, function
// 04) and prints them on the serial console. Poll 2 also shows two other
// things a master meets on a real bus: an exception answer to a register the
// slave does not have, and a single-register write (function 06) to the
// temperature correction register 0x0103.
//
// The printing uses the same UART as the bus. With DE low the console text
// never reaches the A/B pair, which is the same on a real Uno.
#include <Arduino.h>
#include <ModbusMaster.h>

static const uint8_t DE_RE = 2;

static ModbusMaster sensor1;
static ModbusMaster sensor2;

static void preTransmission() { digitalWrite(DE_RE, HIGH); }
static void postTransmission() { digitalWrite(DE_RE, LOW); }

// Tenths of a unit as "-12.5", without floating point.
static void printTenths(int16_t v) {
  if (v < 0) {
    Serial.print('-');
    v = -v;
  }
  Serial.print(v / 10);
  Serial.print('.');
  Serial.print(v % 10);
}

static void poll(const char* name, ModbusMaster& node, uint8_t n) {
  uint8_t r = node.readInputRegisters(0x0001, 2);
  Serial.print("poll ");
  Serial.print(n);
  Serial.print(' ');
  Serial.print(name);
  if (r != node.ku8MBSuccess) {
    Serial.print(" error 0x");
    Serial.println(r, HEX);
    return;
  }
  Serial.print(" T=");
  printTenths((int16_t)node.getResponseBuffer(0));
  Serial.print(" H=");
  printTenths((int16_t)node.getResponseBuffer(1));
  Serial.println();
}

void setup() {
  pinMode(DE_RE, OUTPUT);
  digitalWrite(DE_RE, LOW);  // listen
  Serial.begin(9600);
  sensor1.begin(1, Serial);
  sensor2.begin(2, Serial);
  sensor1.preTransmission(preTransmission);
  sensor1.postTransmission(postTransmission);
  sensor2.preTransmission(preTransmission);
  sensor2.postTransmission(postTransmission);
  Serial.println("modbus-rtu master up");
}

void loop() {
  static uint8_t n = 0;
  n++;
  poll("s1", sensor1, n);
  poll("s2", sensor2, n);
  if (n == 2) {
    // Holding register 0x0200 does not exist on the XY-MD02: exception 02.
    uint8_t r = sensor1.readHoldingRegisters(0x0200, 1);
    Serial.print("poll 2 s1 reg 0x0200 -> 0x");
    Serial.println(r, HEX);
    // Holding register 0x0103 is the temperature correction, in tenths of a
    // degree: +0.5 C on slave 2.
    r = sensor2.writeSingleRegister(0x0103, 5);
    Serial.print("poll 2 s2 write correction -> 0x");
    Serial.println(r, HEX);
  }
  delay(500);
}
