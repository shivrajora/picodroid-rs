#!/usr/bin/env bash
# Refresh the compiled-in TLS trust store, crates/pd-tls/roots/*.der, from the
# host's CA bundle (/etc/ssl/certs, the Mozilla set). Run when a root is
# added below or the bundle rotates one; commit the DER files. The verifier
# anchors at these roots only — intermediates rotate, roots last a decade.
#
#   ./scripts/tls-roots.sh            # rewrite roots/ and print each root
#
# Names are the file stems the generated `Anchor.name` carries.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOTS_DIR="$SCRIPT_DIR/../crates/pd-tls/roots"
BUNDLE="${PICODROID_CA_BUNDLE_DIR:-/etc/ssl/certs}"

# name|bundle file
ROOTS=(
  "gts_root_r1|GTS_Root_R1.pem"
  "gts_root_r4|GTS_Root_R4.pem"
  "isrg_root_x1|ISRG_Root_X1.pem"
  "isrg_root_x2|ISRG_Root_X2.pem"
  "digicert_global_root_g2|DigiCert_Global_Root_G2.pem"
  "digicert_global_root_g3|DigiCert_Global_Root_G3.pem"
  "globalsign_root_r3|GlobalSign_Root_CA_-_R3.pem"
  "globalsign_root_r46|GlobalSign_Root_R46.pem"
  "globalsign_root_e46|GlobalSign_Root_E46.pem"
  "usertrust_rsa|USERTrust_RSA_Certification_Authority.pem"
  "usertrust_ecc|USERTrust_ECC_Certification_Authority.pem"
  "sectigo_root_r46|Sectigo_Public_Server_Authentication_Root_R46.pem"
  "sectigo_root_e46|Sectigo_Public_Server_Authentication_Root_E46.pem"
  "amazon_root_ca_1|Amazon_Root_CA_1.pem"
)

mkdir -p "$ROOTS_DIR"
total=0
for row in "${ROOTS[@]}"; do
  name="${row%%|*}"
  pem="$BUNDLE/${row#*|}"
  out="$ROOTS_DIR/$name.der"
  if [[ ! -f "$pem" ]]; then
    echo "missing in $BUNDLE: ${row#*|}" >&2
    exit 1
  fi
  openssl x509 -in "$pem" -outform DER -out "$out"
  size=$(stat -c %s "$out")
  total=$(( total + size ))
  printf '%-26s %5d B  %s\n' "$name" "$size" \
    "$(openssl x509 -in "$out" -inform DER -noout -subject -enddate | tr '\n' ' ')"
done
echo "total: $total B in $ROOTS_DIR"
