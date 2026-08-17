# ADR-0008: Reversible Security Posture

- Status: Accepted
- Date: 2026-08-17
- Owners: @metaneutrons
- Task: M2-014

## Context

Flash encryption and Secure Boot are stored in eFuses. eFuse bits can be set but
never cleared, so enabling either on a real board is a one-way step taken by the
first boot of a firmware that requests it, not by the build. Release-mode flash
encryption additionally removes the ability of ROM download mode to use the flash
encryption hardware, which takes the cable away as a recovery path.

ADR-0007 reserved the layout for these features and left them off. That is safe
but proves nothing: the configuration was never exercised, and the 57,344-byte
bootloader window rested on reasoning rather than a measurement.

The project needs the capability exercised without any device becoming
permanently different, and the guarantee must survive a careless edit rather than
depend on a comment.

## Decision

Flash encryption is enabled in Development mode together with
`CONFIG_EFUSE_VIRTUAL`. Virtual eFuses make every eFuse operation a simulation,
so no bit is burned. As a second effect, release mode becomes unselectable,
because `SECURE_FLASH_ENCRYPTION_MODE_RELEASE` depends on `!EFUSE_VIRTUAL`.

Because that guarantee is one line in a defaults file, the evidence task enforces
it. `verify_reversible_security_posture` rejects:

- flash encryption or Secure Boot without `CONFIG_EFUSE_VIRTUAL`, which would burn
  a key and an enable bit on the first boot;
- release-mode flash encryption, independently of virtual eFuses;
- NVS encryption through the HMAC scheme, which consumes an eFuse key block.

The manifest records the resulting posture as data and adds the `no-efuse-burn`
and `encryption-not-enforced` exclusions, so a reader cannot mistake a simulated
posture for an enforced one.

NVS encryption uses the flash-encryption scheme. Flash encryption implies NVS
encryption, and ESP-IDF defaults to the HMAC scheme on SoCs with an HMAC
peripheral, whose eFuse key id defaults to `-1` and fails the build. The
flash-encryption scheme instead uses the `nvs_keys` partition that ADR-0007
already reserved and consumes no key block.

Secure Boot stays disabled in the build. Signed binaries require a private key
that must not enter the repository or the evidence bundle.

## Consequences

The build exercises the real configuration, so its cost is known rather than
estimated. The bootloader measures 24,096 bytes without flash encryption, 34,800
bytes with it, and 45,056 bytes with Secure Boot V2 additionally enabled and built
unsigned. The `0x8000` table offset that preceded ADR-0007 gave a 24,576-byte
window, so flash encryption alone would not have fit; that offset move was already
necessary. The current window leaves 22,544 bytes free, and roughly 8 KiB with
Secure Boot and its 4 KiB signature block.

Turning either feature on for real is now a deliberate, review-visible change to
the gate rather than an edit to a configuration line. That is the strongest
guarantee a repository-level control can give; it does not prevent someone from
changing the gate itself.

The posture is not security. With virtual eFuses the encryption is simulated, and
the manifest says so. No claim about confidentiality of flash contents follows
from this change.

Switching NVS encryption to the HMAC scheme later is a configuration change plus
one burned key block and moves no partition. It has a real security advantage,
because the NVS keys would then not rest on flash encryption alone, and it belongs
to the security review before manufacturing.

## Alternatives

Leaving flash encryption disabled was rejected because it leaves the bootloader
window unmeasured and the configuration unexercised, while providing no additional
safety over virtual eFuses.

Enabling flash encryption without virtual eFuses was rejected because the first
board to boot the firmware would burn a key, which the project explicitly does not
want at this stage.

Documenting the reversibility requirement without enforcing it was rejected
because a single deleted line would silently turn a development posture into a
one-way step, and nothing in the pipeline would notice.

Release mode was rejected until the recovery path is decided, because it removes
plaintext recovery over cable and the 16 MB geometry has no room for a third
immutable slot.

## Evidence

`cargo +1.97.1 xtask ci --gate firmware` reports the posture and passes with a
34,800-byte bootloader in a 57,344-byte window.
`cargo +1.97.1 test --locked -p rumiga-xtask` covers the accepted posture, an
absent posture, and each rejected configuration. The Secure Boot measurement used
`SECURE_BOOT_BUILD_SIGNED_BINARIES=n` so that no key was required, and was
reverted rather than committed.

Clean pull-request run
[`32049854368`](https://github.com/metaneutrons/rumiga/actions/runs/32049854368)
produced governance artifact `9294271555` with archive SHA-256
`db92e6e0a918d78368cde2a78f7cd48da2ba19e3f7276a4fb6a63325e04ccff5`. Final `main`
run
[`32065256994`](https://github.com/metaneutrons/rumiga/actions/runs/32065256994)
produced governance artifact `9299639002` with archive SHA-256
`1c5afd8dc30fc599a041bbc7ffa089506c8c89772294003c7b83ae9f2db41acc`. Both were
independently downloaded and verified.

The hosted firmware bundle was also checked directly. Its manifest reports
`efuse_virtual: true` and `burns_efuses: false`, and its resolved `sdkconfig`
confirms `CONFIG_EFUSE_VIRTUAL`, AES-128, the flash-encryption NVS scheme, and the
absence of `CONFIG_SECURE_BOOT`. ESP-IDF additionally resolved
`SECURE_FLASH_UART_BOOTLOADER_ALLOW_ENC`, which is the mechanism that keeps
plaintext flashing over cable available and is precisely why this mode is
reversible and why upstream labels it insecure.

## Supersession

None. This narrows the reservation recorded in ADR-0007 into an enforced
invariant and leaves its layout decision unchanged.
