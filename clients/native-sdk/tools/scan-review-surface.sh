#!/bin/sh
set -eu

script_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd -P)
surface_root=$(CDPATH= cd -- "$script_dir/.." && pwd -P)
scan_glob='!**/scan-review-surface.sh'
failed=0

pass() {
  printf 'PASS %s\n' "$1"
}

fail() {
  printf 'FAIL %s\n' "$1" >&2
  failed=1
}

if find "$surface_root" -type l -print -quit | grep -q .; then
  find "$surface_root" -type l -print >&2
  fail no_symlinks
else
  pass no_symlinks
fi

artifact_matches=$(find "$surface_root" -type f \( \
  -name '*.wasm' -o -name '*.wasm.gz' -o -name '*.exe' -o -name '*.dll' -o \
  -name '*.so' -o -name '*.dylib' -o -name '*.a' -o -name '*.o' -o \
  -name '*.zip' -o -name '*.tar' -o -name '*.tgz' -o -name '*.gz' -o \
  -name '*.7z' -o -name '*.db' -o -name '*.sqlite' -o -name '*.log' \
\) -print)
if [ -n "$artifact_matches" ]; then
  printf '%s\n' "$artifact_matches" >&2
  fail no_binary_or_archive_artifacts
else
  pass no_binary_or_archive_artifacts
fi

large_matches=$(find "$surface_root" -type f -size +1M -print)
if [ -n "$large_matches" ]; then
  printf '%s\n' "$large_matches" >&2
  fail bounded_file_sizes
else
  pass bounded_file_sizes
fi

binary_matches=''
while IFS= read -r path; do
  # Exact approved public raw metadata is the only binary source fixture allowed.
  if [ "$path" = "$surface_root/fixtures/deployed-metadata.scale" ]; then
    actual_hash=$(sha256sum "$path" | cut -d ' ' -f 1)
    if [ "$actual_hash" = "eed011f659bd492aedb643d775fce7cab26adb89c46db8598f4b2c079897e5d2" ]; then
      continue
    fi
  fi
  if ! LC_ALL=C grep -Iq . "$path"; then
    binary_matches="${binary_matches}${path}\n"
  fi
done <<EOF
$(find "$surface_root" -type f -print)
EOF
if [ -n "$binary_matches" ]; then
  printf '%b' "$binary_matches" >&2
  fail text_only_files
else
  pass text_only_files
fi

if rg -n -i -g "$scan_glob" \
  -e 'BEGIN[[:space:]]+(RSA|EC|DSA|OPENSSH|PGP)[[:space:]]+PRIVATE[[:space:]]+KEY' \
  -e 'AKIA[0-9A-Z]{16}' \
  -e 'gh[pousr]_[A-Za-z0-9_]{20,}' \
  -e "(api[_-]?key|password|passwd|secret|access[_-]?token)[[:space:]]*[:=][[:space:]]*[\"']?[A-Za-z0-9/+_.=-]{12,}" \
  "$surface_root"; then
  fail no_likely_secrets
else
  pass no_likely_secrets
fi

# Full alphanumeric boundaries avoid treating a substring of a hex genesis as SS58.
# These two exact account-codec vectors are public Alice and its wrong-prefix test, not owners.
account_matches=$(rg --no-line-number -o -P -g "$scan_glob" -g '!**/fixtures/**' \
  '(?<![A-Za-z0-9])[1-9A-HJ-NP-Za-km-z]{47,49}(?![A-Za-z0-9])' "$surface_root" || true)
unreviewed_accounts=$(printf '%s\n' "$account_matches" | \
  grep -Fvx "$surface_root/tests/account.test.mjs:5GrwvaEF5zXb26Fz9rcQpDWS57CtERHpNehXCPcNoHGKutQY" | \
  grep -Fvx "$surface_root/tests/account.test.mjs:15oF4uVJwmo4TdGW7VvPzCRgDoD8Dp6J9AxvpzDdVhjuySbg" || true)
if [ -n "$unreviewed_accounts" ]; then
  printf '%s\n' "$unreviewed_accounts" >&2
  fail no_unreviewed_account_like_literals
else
  pass no_unreviewed_account_like_literals
fi

if rg -n -g "$scan_glob" -e '/(home|root|srv|var/lib|etc/systemd)/[A-Za-z0-9._/-]+' "$surface_root"; then
  fail no_private_host_paths
else
  pass no_private_host_paths
fi

ipv4_matches=$(rg -n -P -g "$scan_glob" \
  '(?<![0-9])(?:[0-9]{1,3}\.){3}[0-9]{1,3}(?![0-9])' "$surface_root" || true)
external_ipv4=$(printf '%s\n' "$ipv4_matches" | grep -Ev '127\.0\.0\.1' || true)
if [ -n "$external_ipv4" ]; then
  printf '%s\n' "$external_ipv4" >&2
  fail loopback_only_address_literals
else
  pass loopback_only_address_literals
fi

url_matches=$(rg -n -o -g "$scan_glob" 'https?://[^ )`"<>]+' "$surface_root" || true)
external_urls=$(printf '%s\n' "$url_matches" | \
  grep -Ev 'https?://(127\.0\.0\.1|localhost|\[::1\]|example\.invalid)' || true)
if [ -n "$external_urls" ]; then
  printf '%s\n' "$external_urls" >&2
  fail no_external_service_urls
else
  pass no_external_service_urls
fi

if rg -n -i -g '*.md' -g '*.html' \
  -e 'spec[_ ]?version[[:space:]]*[=:][[:space:]]*(10|11|12|13)\b' \
  -e 'specVersion[^0-9]{0,8}(10|11|12|13)\b' \
  "$surface_root"; then
  fail no_stale_runtime_version_claims
else
  pass no_stale_runtime_version_claims
fi

if rg -n -i -P -g "$scan_glob" \
  -e '\b(V14|ERA)\s+(is|are)\s+(?!not\b)(live|audited|production[- ]ready|publicly available)\b' \
  -e '\b(supports|provides|offers)\s+(EVM|Frontier|Solidity|ERC-20|smart contracts?|AMM|DeFi)\b' \
  -e '\b(is|offers?)\s+(a\s+)?guaranteed\s+(APR|yield|return)\b' \
  "$surface_root"; then
  fail no_unsupported_positive_claims
else
  pass no_unsupported_positive_claims
fi

if rg -n -i \
  -e '--validator' -e '--alice' -e '--bob' -e '--charlie' -e '--dave' \
  -e '--eve' -e '--ferdie' -e '--rpc-external' -e '--unsafe-rpc-external' \
  -e '--bootnodes' -e '--reserved-nodes' -e '--public-addr' -e '--discover-local' \
  -e '--rpc-methods[ =]+unsafe' -e 'node-key' -e 'keystore' \
  "$surface_root/node"; then
  fail non_authority_safe_node_template
else
  pass non_authority_safe_node_template
fi

if [ "$failed" -ne 0 ]; then
  printf 'ERA_V14_WS8_SURFACE_SCAN_STATUS=FAIL\n' >&2
  exit 1
fi

file_count=$(find "$surface_root" -type f | wc -l | tr -d ' ')
printf 'FILES_SCANNED=%s\n' "$file_count"
printf 'ERA_V14_WS8_SURFACE_SCAN_STATUS=PASS\n'
