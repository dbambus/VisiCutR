#!/usr/bin/env bash
# Regenerates expected.ltt of every case in tests/java_parity with the
# original Java pipeline (VisiCut + LibLaserCut). Needs Java 17+, Maven, Git
# and network access to Maven Central. The Java source tree comes from the
# tag java-reference (worktree in the temp folder); JAVA_TREE=<dir> uses
# another checkout instead, e.g. one of the fix/* branches.
#
#   bash scripts/java-parity/generate.sh            # all cases
#   bash scripts/java-parity/generate.sh cut_rect   # selected cases
#
# The Java build is reused; REBUILD=1 forces a new one.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
crate="$(cd "$here/../.." && pwd)"
work="${TMPDIR:-/tmp}/visicut-java-parity"
repo="${JAVA_TREE:-$work/java}"
if [ -z "${JAVA_TREE:-}" ] && [ ! -f "$repo/pom.xml" ]; then
  git -C "$crate" worktree add --detach "$repo" java-reference
fi
cases="$crate/tests/java_parity"
jar="$repo/target/visicut-1.9-SNAPSHOT-full.jar"

if [ ! -f "$repo/LibLaserCut/pom.xml" ]; then
  git -C "$repo" submodule update --init LibLaserCut
fi
if [ ! -f "$jar" ] || [ "${REBUILD:-0}" = 1 ]; then
  (cd "$repo/LibLaserCut" && mvn -q -B -DskipTests install)
  # initialize installs the bundled kabeja jars from legacy/.
  (cd "$repo" && mvn -q -B initialize && mvn -q -B -DskipTests package)
fi

rm -rf "$work/classes"
mkdir -p "$work/classes"
javac -cp "$jar" -d "$work/classes" "$here/LttParity.java" "$here/MakePlf.java"

# Same module openings as the Add-Opens entries of VisiCut's jar manifest.
opens=()
for p in java.base/java.util java.base/java.lang java.base/java.io java.base/java.text \
  java.desktop/java.awt.geom java.desktop/java.awt java.xml/com.sun.org.apache.xerces.internal.parsers; do
  opens+=("--add-opens=$p=ALL-UNNAMED")
done

if [ "$#" -gt 0 ]; then
  dirs=()
  for name in "$@"; do dirs+=("$cases/$name"); done
else
  dirs=("$cases"/*/)
fi
# A private home keeps VisiCut from reading or writing the user's settings.
mkdir -p "$work/home"
java "${opens[@]}" -Djava.awt.headless=true -Duser.home="$work/home" \
  -cp "$jar:$work/classes" LttParity "$crate/reference/FAU-LTT-iLaser-4000.xml" "${dirs[@]}"
