# Rename lab – GitKraken 12.7.0 megfigyelések (2026-11-27)

Esetek: `CASES.md`, generátor: `lab.sh`. Minden állítás mögött screenshot + `lab.sh inspect` kimenet van.

## Megjelenítés

- Rename = **egy sor**, kék fájl-ikon, csak az **új** útvonal látszik (Path és Tree nézetben is).
  Tooltip is csak az új útvonal. A fejléc-számláló („N file changes”) és a WIP-jelvény egynek számolja.
- A régi név **csak a diff fejlécében** jelenik meg: `src/OldName.cs ⇒ src/NewName.cs`.
- Diff a régi tartalomhoz képest (rename + 1 sor → csak az 1 sor zöld). Azonos tartalomnál:
  „File contents are unchanged.”
- Commit részletek: összesítő sor `1 renamed` (kék ikon), illetve `+ 1 added − 1 deleted` küszöb alatt.
  Stash részletek ugyanígy (`1 renamed`). (H01, S01)
- A context menü ugyanaz, mint sima fájlnál, nincs rename-specifikus pont. (R01)

## Párosítás

- Küszöb **50%**, mint a Gitnél: 55% rename, 45% törlés + új. Ugyanez commitokban. (R04, R05, H01)
- Pure, mappa-áthelyezés, kis-nagybetű-csere, bináris commitban rename. (H01)
- **Szekción belül párosít, szekciók között nem**:
  - Staged = HEAD ↔ index (ezt `git status` adja).
  - Unstaged = index ↔ munkakönyvtár, **untracked fájlokat is beleértve** (ezt `git status` NEM adja):
    unstaged törlés + untracked új fájl → unstaged rename. (R01, R02)
  - Staged törlés + untracked új fájl → két külön sor (− staged, + unstaged). (R10)
- Ugyanaz a fájl lehet staged rename + unstaged módosítás egyszerre. (R09)

## Szélső esetek

- Mappa-átnevezés (25 fájl): 25 külön rename-sor, nincs összevonás. Tree nézetben a `Modern` mappa
  csukva `🔵 25` jelvénnyel. A 2 szerkesztett fájl (95%, 83%) is rename. (R07)
- Egy törölt, egy azonos és egy közeli másolat: az azonos lesz rename, a közeli sima új fájl. (R12)
- Két azonos tartalmú pár: útvonal-sorrendben párosít (`first→alpha`, `second→beta`). (R13)
- Bináris: azonos tartalom „File contents are unchanged.”, módosított bináris is rename (Git: 97%), a diff
  „New image / Old image” kép-nézet, fejlécben `BINARY`. (R14)
- Üres fájl (`.gitkeep`) **párosul**; 2 soros fájl 1 sor változással (50%) **nem** – mindkettő a Git szerint. (R15)
- Kis-nagybetű-csere stagelve: staged rename, csak az új név (`README.md`). Csak lemezen végzett
  kis-nagybetű-csere (Windows, `core.ignorecase`) nem látszik. (R16)
- LF → CRLF + rename: rename marad, de a diff minden sort cserélt sornak mutat (−20 +20). (R17)
- Felcserélt tartalom két létező útvonalon: két sima módosítás, nincs csere-felismerés (Git `-B` nélkül). (R18)
- WIP-jelvény típusonként számol: `✏1 +1 −1 🔵1`. (R19)
- Rendezés az **új** útvonal szerint: `Zulu.cs → Alpha.cs` a `Mike.cs` elé kerül. (R20)

## Műveletek

- **Stage** egy unstaged rename-soron → a törlést és az új fájlt együtt stageli; indexben valódi `R100`. (R01)
- **Discard** unstaged rename-en → régi név visszaáll, új fájl törlődik. (R01)
- Unstaged rename-en (untracked új fájl) **nincs sor/hunk stage**, se gomb, se menüpont – csak egészben. (R02)
  Sima módosításon van (*Stage Hunk*, *Discard Hunk*, *Stage this line*, *Discard this line*). (R09)
- Stagelt rename diffje helyes, ha nincs mellette unstaged változás (R09 hunk-stage után).

## GitKraken-hibák, nem másoljuk

- Ha a fájl staged rename + unstaged módosítás is, a staged sor diffje `NewName.cs ⇒ NewName.cs`
  fejléccel az egész fájlt újnak mutatja. Helyesen: `OldName.cs ⇒ NewName.cs`, változás nélkül. (R09)

## Mellékes

- Stash *Apply* (S01): a stashben stagelt rename után a Git-állapot unstaged törlés + untracked új fájl
  (semmi sincs stagelve); GK ezt unstaged rename-ként mutatja, és a commit üzenetet a stash üzenetével
  tölti elő (GitCat ezt szándékosan nem teszi, lásd AGENTS.md).
