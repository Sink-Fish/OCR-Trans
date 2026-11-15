#!/usr/bin/env bash
# One-time: create a stable, self-signed code-signing identity "OCR Trans Local Signing" (no admin rights, no
# password prompts). Everything lives in .signing/ (gitignored): the key, the certificate, and a dedicated keychain
# whose random password is stored next to it. The login keychain, the System keychain and trust settings are not
# touched. Idempotent: does nothing if the identity already exists.
#
# Why: ad-hoc signed builds get a new designated requirement (a cdhash) every build, so macOS forgets the Screen
# Recording permission after each update. Signed with this certificate the requirement stays the same
# (identifier "com.sinkfish.ocrtrans" and certificate leaf = H"…"), so the permission sticks.
set -euo pipefail
cd "$(dirname "$0")/.."
D=.signing
KC="$PWD/$D/ocrtrans-signing.keychain-db"
NAME="OCR Trans Local Signing"
if [ -f "$KC" ] && security find-identity "$KC" 2>/dev/null | grep -q "$NAME"; then
  echo "signing identity present: $(security find-identity "$KC" | awk '/'"$NAME"'/{print $2; exit}')"
  exit 0
fi
mkdir -p $D && chmod 700 $D
umask 077
cat > $D/cert.cnf <<CNF
[ req ]
distinguished_name = dn
x509_extensions = ext
prompt = no
[ dn ]
CN = $NAME
[ ext ]
basicConstraints = critical, CA:false
keyUsage = critical, digitalSignature
extendedKeyUsage = critical, codeSigning
subjectKeyIdentifier = hash
CNF
openssl req -x509 -newkey rsa:2048 -nodes -keyout $D/key.pem -out $D/cert.pem -days 3650 -config $D/cert.cnf 2>/dev/null
openssl rand -hex 16 > $D/p12.pass
openssl pkcs12 -export -legacy -inkey $D/key.pem -in $D/cert.pem -name "$NAME" -out $D/identity.p12 -passout file:$D/p12.pass
openssl rand -hex 16 > $D/keychain.pass
rm -f "$KC"
security create-keychain -p "$(cat $D/keychain.pass)" "$KC"
security set-keychain-settings "$KC" # no auto-lock timeout
security unlock-keychain -p "$(cat $D/keychain.pass)" "$KC"
security import $D/identity.p12 -k "$KC" -P "$(cat $D/p12.pass)" -T /usr/bin/codesign
# let codesign use the key without a dialog (our own keychain, so its password is ours to give)
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$(cat $D/keychain.pass)" "$KC" >/dev/null
echo "created signing identity: $(security find-identity "$KC" | awk '/'"$NAME"'/{print $2; exit}')"
echo "back up .signing/ if you want to keep the same identity on another checkout (losing it = one more permission re-grant)"
