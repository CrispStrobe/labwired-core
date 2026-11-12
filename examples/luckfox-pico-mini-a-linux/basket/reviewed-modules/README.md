# Historical module payloads and private original identities

The public directory contains the two reviewed module payloads and
[original-identities.json](original-identities.json), an identity index. It is
**not a complete preparation input**. The intact original `module-manifest.json`
and original build logs contain local environment paths and are retained as
private offline inputs. They are not replaced by redacted receipts. Their byte
counts and hashes remain historical commitments; the index is not proof of fresh
preparation, original-byte availability or guest acceptance.

The basket preparer still requires the exact original receipt, 64,632 bytes /
`f5fab55c5d3cecb9713d277b9c19f3b215d781467ad91971219e35cf8e503ab6`,
before checking exact module bytes, ELF, vermagic, CRCs, signatures and
source/package/preparer provenance. Every existing identity pin and preparer
source is unchanged. A fresh or redacted receipt must fail this original-byte
gate, even if its module payloads match. Fresh producer receipts must retain their
actual observations privately.

An operator who already holds the original inputs can supply an offline directory
containing the intact receipt and matching module payloads to `--module-artifacts`.
First run `python3 scripts/tests/qualify_basket_original.py --module-artifacts
<offline-original-directory>` to check their exact identities. This is an input
check, not the affected preparation or guest qualification. Missing original bytes
remain a blocker; no download, reconstruction from repository history, rewritten
receipt or automatic fallback is provided.

The public hosted basket job explicitly fails before checkout or any preparation,
private app access, download, build or guest run. Its raw artifact uploader has
been removed. A private input/execution/evidence route requires separate review;
this change does not establish one or qualify a guest. Prior repository history
and previously published artifacts are unchanged.

Both modules remain GPL-2.0-only. Exact matching source/package identities and
reproduction requirements remain documented in
[DRIVER_MODULES.md](../../DRIVER_MODULES.md) and
[prepare-rv1103-modules.py](../../../../scripts/prepare-rv1103-modules.py).
Original command and log digests identify original observations, not a current
build or a licence-clearance claim.
