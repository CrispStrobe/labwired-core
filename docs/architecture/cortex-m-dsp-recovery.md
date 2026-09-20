# Cortex-M DSP multiply recovery

Source: CrispStrobe/labwired-core commit `f1d2705d1664060eb27f5c4f67a88e9763582d31`
by Claude <noreply@anthropic.com>. This port adapts its multiply semantics to the
current split decoder/executor. It is not a cherry-pick of the broad DSP patch.

## Scope and audit against upstream 11dc78968

| Fork family | Current upstream | Recovery |
| --- | --- | --- |
| SMULxy / SMLAxy | Already decoded and executed as SmlaXy | Preserve; correct missing sticky Q on signed accumulation overflow |
| SMULWy / SMLAWy | Missing | Recover both bottom/top forms |
| SMUAD/X / SMUSD/X / SMLAD/X / SMLSD/X | Missing | Recover eight forms |
| SMMUL/R / SMMLA/R / SMMLS/R | Missing | Recover six forms |
| SMLALxy | Missing | Recover all four selectors |
| SMLALD/X / SMLSLD/X | Missing | Recover four forms |
| SSAT / USAT / SSAT16 / USAT16 | Partial raw USAT fallback | Deferred as a separate decode audit; fork mask leaves reserved saturation bits unconstrained |
| Hint-space and unknown-instruction changes | Upstream models event register; a stale raw branch catch-all still skips FBxx unknown multiplies | Exclude multiply space from that catch-all so invalid DSP faults; leave hint behavior unchanged |

New decodes reject reserved op2 bits, SP/PC operand encodings, SMMLS with Ra=15,
and equal long-result register pairs. Unsupported USAD8 remains unknown.
NZCV/GE are unchanged. Q is sticky, set only by the appropriate 32-bit
accumulating operations (and SMUAD), never by top-word/long operations.
Dual multiply overflow is checked on the final mathematical sum, allowing a
negative accumulator to cancel an overflowing intermediate product sum.

## Reference and independent opcode evidence

Semantics checked against the Arm Architecture Reference Manual, including
[SMLAD operation](https://documentation-service.arm.com/static/5f8daeb7f86e16515cdb8c4e)
and [assembler instruction descriptions](https://documentation-service.arm.com/static/5f3fa899428f7a6b3328fd44).
These are architectural tests, not physical hardware validation.

Generate independent instruction encodings with:

```sh
arm-none-eabi-as -mcpu=cortex-m4 -mthumb dsp.s -o dsp.o
arm-none-eabi-objdump -d dsp.o
```

Use `.syntax unified`, `.thumb`, `.cpu cortex-m4`, followed by:

```asm
.syntax unified
.thumb
.cpu cortex-m4
smulwb r0,r1,r2
smulwt r0,r1,r2
smlawb r0,r1,r2,r3
smlawt r0,r1,r2,r3
smuad r0,r1,r2
smuadx r0,r1,r2
smusd r0,r1,r2
smusdx r0,r1,r2
smlad r0,r1,r2,r3
smladx r0,r1,r2,r3
smlsd r0,r1,r2,r3
smlsdx r0,r1,r2,r3
smmul r0,r1,r2
smmulr r0,r1,r2
smmla r0,r1,r2,r3
smmlar r0,r1,r2,r3
smmls r0,r1,r2,r3
smmlsr r0,r1,r2,r3
smlalbb r0,r3,r1,r2
smlalbt r0,r3,r1,r2
smlaltb r0,r3,r1,r2
smlaltt r0,r3,r1,r2
smlald r0,r3,r1,r2
smlaldx r0,r3,r1,r2
smlsld r0,r3,r1,r2
smlsldx r0,r3,r1,r2
usad8 r0,r1,r2

```

## Verification plan

1. Add failing execution tests for the assembler-generated opcodes and Q overflow.
2. Port only missing multiply families with precise masks and operand validation.
3. Exercise selector combinations, signed halves, exchange, final-sum overflow
   cancellation, 64-bit wrapping, rounding, aliased destinations and sticky Q.
4. Run focused Cortex-M/decoder tests and verify unsupported encodings remain
   unknown and fault. Preserve existing event and exception behavior.
