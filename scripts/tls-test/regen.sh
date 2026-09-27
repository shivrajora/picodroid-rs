#!/usr/bin/env bash
# Regenerate the TEST-ONLY certificates the nightly's TLS listener serves
# (scripts/tls-listener.py) and that a test build trusts through
# PICODROID_TLS_EXTRA_CA=scripts/tls-test/test-ca.der. All P-256:
#
#   test-ca.{pem,der}, test-ca-key.pem   the CA, ten years
#   localhost.pem, localhost-key.pem     a leaf for localhost / picodroid-test /
#                                        127.0.0.1, ten years           (port 8443)
#   expired.pem, expired-key.pem         the same names, valid one day in 2020
#                                                                        (port 8444)
#   untrusted.pem, untrusted-key.pem     self-signed, under no CA        (port 8445)
#   wrongname.pem, wrongname-key.pem     a leaf for other.example only   (port 8446)
#
# The private keys are committed on purpose: they secure nothing but a
# loopback test — never use them for anything else. Rerun when a leaf
# expires; commit the outputs.
set -euo pipefail
cd "$(dirname "$0")"

SAN="DNS:localhost,DNS:picodroid-test,IP:127.0.0.1"

# The CA.
openssl ecparam -name prime256v1 -genkey -noout -out test-ca-key.pem
openssl req -x509 -new -key test-ca-key.pem -sha256 -days 3650 \
  -subj "/O=picodroid test/CN=picodroid test CA" \
  -addext "basicConstraints=critical,CA:TRUE" \
  -addext "keyUsage=critical,keyCertSign,cRLSign" \
  -out test-ca.pem
openssl x509 -in test-ca.pem -outform DER -out test-ca.der

# A leaf under the CA: $1 name, $2 CN, $3 SAN, $4 days (or "expired").
# `openssl ca` rather than `x509 -req` because only it takes explicit dates
# (OpenSSL 3.0).
mint() {
  local name="$1" cn="$2" san="$3" days="$4"
  openssl ecparam -name prime256v1 -genkey -noout -out "$name-key.pem"
  openssl req -new -key "$name-key.pem" -sha256 -subj "/O=picodroid test/CN=$cn" -out "$name.csr"
  local work
  work="$(mktemp -d)"
  cat > "$work/ca.cnf" <<EOF
[ ca ]
default_ca = test
[ test ]
dir = $work
database = $work/index.txt
serial = $work/serial
new_certs_dir = $work
certificate = $PWD/test-ca.pem
private_key = $PWD/test-ca-key.pem
default_md = sha256
policy = any
x509_extensions = leaf
copy_extensions = none
unique_subject = no
[ any ]
commonName = supplied
organizationName = optional
[ leaf ]
basicConstraints = CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = serverAuth
subjectAltName = $san
EOF
  touch "$work/index.txt"
  openssl rand -hex 8 > "$work/serial"
  local -a when
  if [[ "$days" == "expired" ]]; then
    when=(-startdate 20200101000000Z -enddate 20200102000000Z)
  else
    when=(-days "$days")
  fi
  openssl ca -batch -notext -config "$work/ca.cnf" -in "$name.csr" -out "$name.pem" "${when[@]}" \
    2>&1
  rm -rf "$work" "$name.csr"
}

mint localhost localhost "$SAN" 3650
mint expired localhost "$SAN" expired
mint wrongname other.example "DNS:other.example" 3650

# Self-signed: no CA in any trust store.
openssl ecparam -name prime256v1 -genkey -noout -out untrusted-key.pem
openssl req -x509 -new -key untrusted-key.pem -sha256 -days 3650 \
  -subj "/O=picodroid test/CN=localhost" -addext "subjectAltName=$SAN" -out untrusted.pem

for f in test-ca localhost expired wrongname untrusted; do
  printf '%-10s %s\n' "$f" "$(openssl x509 -in "$f.pem" -noout -subject -enddate | tr '\n' ' ')"
done
