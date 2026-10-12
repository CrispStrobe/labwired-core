// LabWired B9 probe: ESP32 LEDC duty on the pad, via stock Arduino-ESP32.
//
// GPIO2: analogWrite (Arduino's default 1 kHz / 8-bit LEDC channel).
// GPIO5: ledcSetup(0, 5000, 10) + ledcAttachPin(5, 0) + ledcWrite(0, ..) —
//        a second LEDC channel (HS group) on another timer configuration;
//        analogWrite takes the last channel (LS group).
// GPIO4: plain digitalWrite phase marker, toggled after each duty change so
//        a host can cut the trace into phases without a UART.
//
// Phases (GPIO2 duty /255, GPIO5 duty /1023):
//   0: 0 %     , 100 % (ledcWrite 1023 = full on: Arduino maps max to 2^res)
//   1: 64      , 256
//   2: 128     , 768
//   3: 191     , 0
//   4: 255     , 512
#include <Arduino.h>

static const int PWM_A = 2;
static const int PWM_B = 5;
static const int PHASE = 4;

static const int DUTY_A[] = {0, 64, 128, 191, 255};
static const int DUTY_B[] = {1023, 256, 768, 0, 512};

void setup() {
  Serial.begin(115200);
  pinMode(PHASE, OUTPUT);
  digitalWrite(PHASE, LOW);
  ledcSetup(0, 5000, 10);
  ledcAttachPin(PWM_B, 0);
  for (int i = 0; i < 5; i++) {
    analogWrite(PWM_A, DUTY_A[i]);
    ledcWrite(0, DUTY_B[i]);
    digitalWrite(PHASE, (i & 1) ? LOW : HIGH);
    Serial.printf("LW_B9_PHASE %d %d %d\n", i, DUTY_A[i], DUTY_B[i]);
    delay(6);
  }
  digitalWrite(PHASE, LOW);
  Serial.println("LW_B9_DONE");
}

void loop() {
  delay(1);
}
