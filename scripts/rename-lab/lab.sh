#!/usr/bin/env bash
# GitCat rename lab: builds throwaway repositories that reproduce every rename
# shape we want to compare against GitKraken (unstaged, staged, half-staged,
# similarity thresholds, binary, case-only, history and stash).
#
#   bash scripts/rename-lab/lab.sh build            # (re)build every case
#   bash scripts/rename-lab/lab.sh build R02 R05    # (re)build selected cases
#   bash scripts/rename-lab/lab.sh list             # ids, paths, one-line summary
#   bash scripts/rename-lab/lab.sh inspect R02      # dump Git state to paste back
#
# LAB_ROOT overrides the output folder (default C:/tmp/gitcat-rename-lab).
# Close the case's tab in GitKraken/GitCat before rebuilding it: Windows keeps
# open files locked.
set -euo pipefail

LAB_ROOT="${LAB_ROOT:-/c/tmp/gitcat-rename-lab}"

export GIT_AUTHOR_NAME="Lab" GIT_AUTHOR_EMAIL="lab@lab.invalid"
export GIT_COMMITTER_NAME="Lab" GIT_COMMITTER_EMAIL="lab@lab.invalid"
export GIT_EDITOR=true GIT_TERMINAL_PROMPT=0

# id|slug|summary
CASES=(
  "R01|typo-pair|unstaged: Views/CaputrePage.xaml(.cs) -> CapturePage.xaml(.cs), identical content (your screenshot)"
  "R02|rename-plus-line|unstaged: rename + one line appended (~95% similar)"
  "R03|rename-70|unstaged: rename + 6 of 20 lines changed (~70% similar)"
  "R04|rename-55|unstaged: rename + 9 of 20 lines changed (~55%, just above Git's 50%)"
  "R05|rename-45|unstaged: rename + 11 of 20 lines changed (~45%, just below Git's 50%)"
  "R06|move-dir|unstaged: Views/Settings.xaml -> Pages/Settings.xaml (other folder, same name)"
  "R07|folder-rename|unstaged: whole folder Legacy/ (25 files) -> Modern/, 2 of them edited"
  "R08|staged-pure|staged: git mv old -> new, nothing else"
  "R09|staged-plus-edit|staged git mv + extra UNSTAGED edit in the new file"
  "R10|half-del-staged|deletion staged (git rm), new file still untracked"
  "R11|half-add-staged|new file staged (git add), deletion only in worktree"
  "R12|ambiguous|one file deleted, an exact copy and a near copy both added"
  "R13|duplicates|two identical files deleted, two identical files added"
  "R14|binary|logo.png renamed exact, icon.png renamed + bytes changed"
  "R15|tiny-empty|empty .gitkeep moved; 2-line file renamed with 1 line changed"
  "R16|case-only|Readme.md -> README.md staged (git mv); Notes.md -> notes.md only on disk"
  "R17|crlf|rename + whole file converted LF -> CRLF"
  "R18|swap|a.txt and b.txt swapped contents (both paths still exist)"
  "R19|rename-and-modify-other|unstaged rename next to ordinary modified/added/deleted files"
  "R20|sort-order|unstaged: Zulu.cs -> Alpha.cs next to modified Mike.cs (which name sorts the row?)"
  "H01|history|clean worktree; one commit per rename shape (pure, 70%, 45%, dir, case, binary)"
  "S01|stash|stash holding a staged rename + edit; worktree clean"
)

N=0
cm() {
  N=$((N + 1))
  local d
  d=$(printf '2026-11-01T%02d:%02d:00+01:00' $((8 + N / 60)) $((N % 60)))
  git add -A
  GIT_AUTHOR_DATE=$d GIT_COMMITTER_DATE=$d git commit -q -m "$1"
}
cfg() {
  git -C "$1" config core.autocrlf false
  git -C "$1" config core.safecrlf false
  git -C "$1" config commit.gpgsign false
  git -C "$1" config user.name Lab
  git -C "$1" config user.email lab@lab.invalid
}
fresh() { rm -rf "$1"; mkdir -p "$1"; }
init_repo() { fresh "$1"; git -C "$1" init -q -b main; cfg "$1"; cd "$1"; }

# Twenty equal-length lines unique to $2, so changing k lines makes the file
# exactly (20-k)/20 similar and no two generated files resemble each other.
body() { # $1 file, $2 tag, $3 line count (default 20)
  local f=$1 tag=$2 n=${3:-20} i
  mkdir -p "$(dirname "$f")"
  for i in $(seq 1 "$n"); do
    printf '    <Row Id="%02d" Key="%s-%02d" Value="original" />\n' "$i" "$tag" "$i"
  done >"$f"
}
chg() { # $1 file, rest: line numbers to rewrite (same length)
  local f=$1; shift
  local n; for n in "$@"; do sed -i "${n}s/original/CHANGED!/" "$f"; done
}
mvp() { mkdir -p "$(dirname "$2")"; mv "$1" "$2"; }
pngish() { mkdir -p "$(dirname "$1")"; { printf '\211PNG\r\n\032\n'; head -c 3000 /dev/zero | tr '\0' "$2"; printf '\000\001\377'; } >"$1"; }

xaml_pair() { # $1 folder, $2 page name
  body "$1/$2.xaml" "$2-xaml"
  body "$1/$2.xaml.cs" "$2-code"
}

base() { # a few unrelated files so the panel is not empty of context
  body App.xaml app
  body README.txt readme 8
}

# ---------------------------------------------------------------- unstaged

case_R01() {
  init_repo "$1"; base
  xaml_pair Catchy/Views CaputrePage
  body Catchy/Views/MainPage.xaml main
  cm "base"
  mv Catchy/Views/CaputrePage.xaml Catchy/Views/CapturePage.xaml
  mv Catchy/Views/CaputrePage.xaml.cs Catchy/Views/CapturePage.xaml.cs
}

case_R02() {
  init_repo "$1"; base
  body src/OldName.cs oldname; cm "base"
  mv src/OldName.cs src/NewName.cs
  echo '    // one line added after the rename' >>src/NewName.cs
}

case_R03() {
  init_repo "$1"; base
  body src/Report.cs report; cm "base"
  mv src/Report.cs src/Summary.cs
  chg src/Summary.cs 2 5 8 11 14 17
}

case_R04() {
  init_repo "$1"; base
  body src/Report.cs report; cm "base"
  mv src/Report.cs src/Summary.cs
  chg src/Summary.cs 1 3 5 7 9 11 13 15 17
}

case_R05() {
  init_repo "$1"; base
  body src/Report.cs report; cm "base"
  mv src/Report.cs src/Summary.cs
  chg src/Summary.cs 1 3 5 7 9 11 13 15 17 19 20
}

case_R06() {
  init_repo "$1"; base
  body Views/Settings.xaml settings; cm "base"
  mvp Views/Settings.xaml Pages/Settings.xaml
}

case_R07() {
  init_repo "$1"; base
  local i
  for i in $(seq -w 1 25); do body "Legacy/Part$i.cs" "part$i" 12; done
  cm "base"
  mv Legacy Modern
  echo '    // edited after the move' >>Modern/Part03.cs
  chg Modern/Part17.cs 2 4
}

# ---------------------------------------------------------------- staged / half

case_R08() {
  init_repo "$1"; base
  body src/OldName.cs oldname; cm "base"
  git mv src/OldName.cs src/NewName.cs
}

case_R09() {
  init_repo "$1"; base
  body src/OldName.cs oldname; cm "base"
  git mv src/OldName.cs src/NewName.cs
  echo '    // unstaged edit after the staged rename' >>src/NewName.cs
}

case_R10() {
  init_repo "$1"; base
  body src/OldName.cs oldname; cm "base"
  cp src/OldName.cs src/NewName.cs
  git rm -q src/OldName.cs
}

case_R11() {
  init_repo "$1"; base
  body src/OldName.cs oldname; cm "base"
  cp src/OldName.cs src/NewName.cs
  git add src/NewName.cs
  rm src/OldName.cs
}

# ---------------------------------------------------------------- pairing

case_R12() {
  init_repo "$1"; base
  body data/source.txt source; cm "base"
  cp data/source.txt data/copy-exact.txt
  cp data/source.txt data/copy-near.txt
  echo '    <!-- near copy: one extra line -->' >>data/copy-near.txt
  rm data/source.txt
}

case_R13() {
  init_repo "$1"; base
  body data/first.txt twin; cp data/first.txt data/second.txt; cm "base"
  mv data/first.txt data/alpha.txt
  mv data/second.txt data/beta.txt
}

case_R14() {
  init_repo "$1"; base
  pngish img/logo.png 'L'; pngish img/icon.png 'I'; cm "base"
  mv img/logo.png img/brand.png
  mv img/icon.png img/app-icon.png
  printf 'XXXXXXXXXXXXXXXX' | dd of=img/app-icon.png bs=1 seek=100 conv=notrunc status=none
}

case_R15() {
  init_repo "$1"; base
  mkdir -p old && : >old/.gitkeep
  printf 'first line\nsecond line\n' >short.txt
  cm "base"
  mvp old/.gitkeep new/.gitkeep
  printf 'first line\nsecond line edited\n' >brief.txt
  rm short.txt
}

case_R16() {
  init_repo "$1"; base
  body Readme.md readmemd 6; body Notes.md notes 6; cm "base"
  git mv Readme.md README.md
  mv Notes.md tmp-case && mv tmp-case notes.md
}

case_R17() {
  init_repo "$1"; base
  body src/Unix.cs unix; cm "base"
  mv src/Unix.cs src/Windows.cs
  sed -i 's/$/\r/' src/Windows.cs
}

case_R18() {
  init_repo "$1"; base
  body a.txt aaa; body b.txt bbb; cm "base"
  mv a.txt tmp-swap && mv b.txt a.txt && mv tmp-swap b.txt
}

case_R19() {
  init_repo "$1"; base
  body src/OldName.cs oldname; body src/Keep.cs keep; body src/Gone.cs gone; cm "base"
  mv src/OldName.cs src/NewName.cs
  chg src/Keep.cs 4
  rm src/Gone.cs
  body src/Brand.cs brand
}

case_R20() {
  init_repo "$1"; base
  body src/Zulu.cs zulu; body src/Mike.cs mike; cm "base"
  mv src/Zulu.cs src/Alpha.cs
  chg src/Mike.cs 4
}

# ---------------------------------------------------------------- history / stash

case_H01() {
  init_repo "$1"; base
  body src/Pure.cs pure; body src/Seventy.cs seventy; body src/FortyFive.cs fortyfive
  body Views/Moved.xaml moved; body Readme.md readmemd 6
  pngish img/logo.png 'L'
  cm "base"
  git mv src/Pure.cs src/PureRenamed.cs; cm "rename: pure (100%)"
  git mv src/Seventy.cs src/SeventyRenamed.cs; chg src/SeventyRenamed.cs 2 5 8 11 14 17
  cm "rename: + 6/20 lines changed (~70%)"
  git mv src/FortyFive.cs src/FortyFiveRenamed.cs; chg src/FortyFiveRenamed.cs 1 3 5 7 9 11 13 15 17 19 20
  cm "rename: + 11/20 lines changed (~45%, below threshold)"
  mkdir -p Pages; git mv Views/Moved.xaml Pages/Moved.xaml; cm "rename: move to other folder"
  git mv Readme.md README.md; cm "rename: case only"
  git mv img/logo.png img/brand.png; cm "rename: binary"
}

case_S01() {
  init_repo "$1"; base
  body src/OldName.cs oldname; cm "base"
  git mv src/OldName.cs src/NewName.cs
  echo '    // edit stored with the rename' >>src/NewName.cs
  git add -A
  git stash push -q -m "rename OldName -> NewName + edit"
}

# ---------------------------------------------------------------- driver

row() { local c; for c in "${CASES[@]}"; do [[ ${c%%|*} == "$1" ]] && { echo "$c"; return; }; done; return 1; }
case_dir() { local r; r=$(row "$1") || { echo "unknown case $1" >&2; exit 2; }; IFS='|' read -r id slug _ <<<"$r"; echo "$LAB_ROOT/$id-$slug"; }

build() {
  local id d
  mkdir -p "$LAB_ROOT"
  for id in "$@"; do
    d=$(case_dir "$id")
    printf '%-4s %s ... ' "$id" "$d"
    ( "case_$id" "$d" ) >"$LAB_ROOT/.build-$id.log" 2>&1 && echo ok || { echo FAILED; return 1; }
  done
}

inspect() {
  local d; d=$(case_dir "$1")
  cd "$d"
  echo "=== $1  $d"
  echo "--- status (what GitCat reads today)"
  git status --porcelain=v2 --branch --untracked-files=all
  echo "--- staged side (index vs HEAD, -M)"
  git diff --cached -M --name-status
  echo "--- Git's verdict if everything were staged (temporary index, -M)"
  local tmp; tmp=$(mktemp)
  cp "$(git rev-parse --git-path index)" "$tmp"
  GIT_INDEX_FILE=$tmp git add -A
  GIT_INDEX_FILE=$tmp git diff --cached -M --name-status
  rm -f "$tmp"
  echo "--- stash"; git stash list
  git stash list -q >/dev/null 2>&1 && git stash show -M --name-status 2>/dev/null || true
  echo "--- history (-M)"
  git log --format='%h %s' -M --name-status -12
}

cmd=${1:-help}; shift || true
case $cmd in
  build)
    [[ $# -eq 0 ]] && set -- $(for c in "${CASES[@]}"; do echo "${c%%|*}"; done)
    build "$@" ;;
  list) for c in "${CASES[@]}"; do IFS='|' read -r id _ s <<<"$c"; printf '%-4s %-52s %s\n' "$id" "$(cygpath -w "$(case_dir "$id")")" "$s"; done ;;
  inspect) for id in "$@"; do inspect "$id"; done ;;
  *) sed -n '2,13p' "$0" ;;
esac
