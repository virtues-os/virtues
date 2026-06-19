# WireGuard on NVIDIA Jetson (Tegra kernels)

The installer stopped with *"WireGuard kernel module missing"*, `virtues doctor`
shows `wireguard: ✗ kernel support missing`, or `virtues-wireguard` logged
*"kernel WireGuard unavailable … see docs/jetson-wg.md"*. This page gets WireGuard
working on a Jetson. **You only need this on Jetson/Tegra** — stock Ubuntu/Debian
(and the DIY mini-PC floor) ship WireGuard already. (If you only want a LAN-only
box, re-run the installer with `--local` and skip all of this.)

## Why it's missing

WireGuard has been in the mainline Linux kernel since 5.6, and JetPack 6 ships a
5.15 kernel — so it *should* be a one-line `modprobe`. But NVIDIA builds their own
"Tegra" kernel with a custom config and leaves `CONFIG_WIREGUARD` **off**, along
with two crypto-library pieces WireGuard needs (`chacha20poly1305` and `poly1305`
libs). And the usual fallback, `wireguard-dkms`, is for kernels *older* than 5.6 —
it refuses to build here. So the module has to be built from source against the
running kernel.

Nothing about Virtues is involved — any WireGuard / Tailscale-kernel-mode / modern
VPN hits this same wall on a stock Jetson.

## Build it (≈5 min)

Run on the box. Replace `5.15.148` with your exact version if `uname -r` differs.

```bash
set -e
KREL=$(uname -r)                                   # e.g. 5.15.148-tegra
sudo apt-get install -y nvidia-l4t-kernel-headers build-essential
ls -ld /lib/modules/$KREL/build                    # must resolve to the headers

# Fetch matching mainline source (WG + crypto libs are self-contained here).
cd /tmp
wget -q https://cdn.kernel.org/pub/linux/kernel/v5.x/linux-5.15.148.tar.xz
tar xf linux-5.15.148.tar.xz

# 1) Build the two missing crypto-lib modules.
cd /tmp/linux-5.15.148/lib/crypto
make -C /lib/modules/$KREL/build M=$PWD \
  CONFIG_CRYPTO_LIB_CHACHA=m \
  CONFIG_CRYPTO_LIB_POLY1305_GENERIC=m \
  CONFIG_CRYPTO_LIB_POLY1305=m \
  CONFIG_CRYPTO_LIB_CHACHA20POLY1305=m \
  modules

EX=/lib/modules/$KREL/extra
sudo install -D -m644 libpoly1305.ko        $EX/libpoly1305.ko
sudo install -D -m644 libchacha20poly1305.ko $EX/libchacha20poly1305.ko

# 2) Build wireguard.ko, pointing modpost at the new crypto symbols
#    (KBUILD_EXTRA_SYMBOLS is required, or it errors on undefined chacha20poly1305_*).
cd /tmp/linux-5.15.148/drivers/net/wireguard
make -C /lib/modules/$KREL/build M=$PWD CONFIG_WIREGUARD=m \
  KBUILD_EXTRA_SYMBOLS=/tmp/linux-5.15.148/lib/crypto/Module.symvers modules
sudo install -D -m644 wireguard.ko $EX/wireguard.ko

# 3) Register, load, persist across reboots.
sudo depmod -a
sudo modprobe wireguard
echo wireguard | sudo tee /etc/modules-load.d/wireguard.conf

# 4) Prove it.
sudo ip link add wgtest type wireguard && sudo ip link del wgtest && echo "WG SUPPORTED"
```

> The `gcc differs from the one used to build the kernel` warning is harmless —
> the module's vermagic matches because it's built against the kernel's own
> `build/` tree.

## Stable address (do this too)

A server wants a stable IPv6, not the rotating SLAAC "privacy" address — otherwise
the box bakes an address into pairing bundles that expires in ~24h. Disable
temporary addresses and drop any lingering one:

```bash
printf 'net.ipv6.conf.all.use_tempaddr=0\nnet.ipv6.conf.default.use_tempaddr=0\n' \
  | sudo tee /etc/sysctl.d/99-virtues-stable-v6.conf
sudo sysctl --system
# delete any existing `... scope global temporary ...` address (find it with:
#   ip -6 addr show scope global temporary)
# sudo ip -6 addr del <that-address>/64 dev <iface>
```

## Finish

```bash
sudo systemctl restart virtues-wireguard
sudo wg show                 # expect: interface wg0, listening port 51820
virtues doctor               # expect: wireguard ✓
```

The `virtues-wireguard` daemon slow-retries while WG is absent, so it picks the
module up on its own after step 3 — the restart just makes it immediate.

## Reboots & kernel bumps

`/etc/modules-load.d/wireguard.conf` reloads the module every boot. But the
modules are pinned to this **exact** kernel version — if a JetPack/L4T update
bumps the kernel, re-run this build against the new `uname -r`. The durable fix
(tracked separately) is enabling `CONFIG_WIREGUARD=m` + the crypto libs in the
appliance image's kernel config so none of this is needed.
