#!/bin/sh
# Refuses a commit that would add library data: rekordbox databases, backups,
# analysis files, kfbox's history, exports, audio, secrets, or anything large.
# It reads the staged names, so `git add -f` doesn't get past it. The one
# exception is the made-up click tracks in crates/kfbox-core/tests/fixtures/.
bad=$(git diff --cached --name-only --diff-filter=ACMR | grep -v '^crates/kfbox-core/tests/fixtures/click-[a-z]*\.mp3$' | grep -Ei \
  '\.(db|db-wal|db-shm|db-journal|edb|dat|ext|2ex|3ex|xml|mp3|wav|aif|aiff|flac|m4a|mp4|aac|ogg|zip)$|(^|/)(backups?|history|USBANLZ|PIONEER)/|(^|/)\.env')
if [ -n "$bad" ]; then
  echo "guard-staged: these must not be committed:" >&2
  echo "$bad" >&2
  exit 1
fi
big=$(git diff --cached --name-only --diff-filter=ACMR | while IFS= read -r f; do
  size=$(git cat-file -s ":$f" 2>/dev/null || echo 0)
  [ "$size" -gt 2000000 ] && echo "$f"
done)
if [ -n "$big" ]; then
  echo "guard-staged: over 2 MB, so probably not source:" >&2
  echo "$big" >&2
  exit 1
fi
exit 0
