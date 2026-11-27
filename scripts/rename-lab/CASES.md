# Rename lab – GitKraken összehasonlító protokoll

Generátor: `scripts/rename-lab/lab.sh` (Git Bash). Kimenet: `C:\tmp\gitcat-rename-lab\`.

```bash
bash scripts/rename-lab/lab.sh build          # minden eset újra
bash scripts/rename-lab/lab.sh build R02      # egy eset újra (előtte zárd be a tabot GK-ban!)
bash scripts/rename-lab/lab.sh list           # id + megnyitandó mappa
bash scripts/rename-lab/lab.sh inspect R02    # Git-állapot dump, ezt másold vissza
```

Minden eset önálló repo, közvetlenül a kiírt mappát nyisd meg GitKrakenben
(*File → Open repo*). A fájlok tartalma szándékosan 20 egyforma hosszú sor, így a
„hány sor változott” pontosan megadja a hasonlóságot (6/20 változás = 70%).
Git alapküszöbe 50%: az `inspect` „Git's verdict” blokkja mutatja, mit mondana a Git.

## Alapcsomag – minden esetnél ezt kérem

1. **Jobb panel (WIP sor kijelölve)**, *Path* és *Tree* nézetben is (a panel tetején a
   lista/fa kapcsoló). Látszódjon az ikon (R / + / − / ceruza), a fájlszám a szekciók
   fejlécén, és ha van, a régi név (`régi → új`).
2. **Hover a fájlon**: ha tooltip jön (régi útvonal, hasonlóság), arról is kép.
3. **Kattintás a fájlra**: a diff nézet képe. Fontos: a régihez képest mutat diffet
   (csak a változott sorok), vagy az egész fájlt zöldnek? Fejlécben mi a cím?
4. **`inspect <id>` kimenete** a lépés előtt – egyszer elég esetenként.

Ahol alább **Művelet** szerepel, azt az alapcsomag *után* csináld, és utána is kérek
képet a panelről + `inspect` kimenetet. Egy művelet után az eset elrontódik:
a következő művelet előtt `build <id>` (tabot előtte bezárni).

## R – munkakönyvtár (WIP) esetek

| ID | Kiinduló állapot | Művelet(ek) | Fő kérdés |
| --- | --- | --- | --- |
| R01<br>`C:\tmp\gitcat-rename-lab\R01-typo-pair` | `Catchy/Views/CaputrePage.xaml(.cs)` → `CapturePage.*`, tartalom azonos, semmi sincs stagelve | **a)** *Stage* az egyik fájlon. **b)** (újraépítés) *Stage all changes*. **c)** (újraépítés) jobb klikk → *Discard changes* az egyik fájlon | a te screenshotod; stage egy rename-sort: mindkét oldal (törlés + új) bekerül? discard visszahozza a régi nevet és törli az újat? |
| R02<br>`C:\tmp\gitcat-rename-lab\R02-rename-plus-line` | rename + 1 sor hozzáadva (Git: 96%) | **a)** kattints a fájlra, a diffben stageld **csak a hozzáadott sort** (sor/hunk stage). **b)** (újraépítés) *Stage* az egész fájlon | rename + módosítás diffje; engedi-e a sor-szintű stage-et; mi lesz a panelen utána (félig stagelt rename) |
| R03<br>`C:\tmp\gitcat-rename-lab\R03-rename-70` | rename + 6/20 sor (70%) | – | még rename-ként mutatja? |
| R04<br>`C:\tmp\gitcat-rename-lab\R04-rename-55` | rename + 9/20 sor (55%) | – | küszöb felett: rename? |
| R05<br>`C:\tmp\gitcat-rename-lab\R05-rename-45` | rename + 11/20 sor (45%) | – | küszöb alatt: törlés + új fájl? (ha R04 rename és R05 nem, GK is 50%-ot használ) |
| R06<br>`C:\tmp\gitcat-rename-lab\R06-move-dir` | `Views/Settings.xaml` → `Pages/Settings.xaml` | – | Tree nézetben melyik mappában jelenik meg, és mutatja-e a régi mappát |
| R07<br>`C:\tmp\gitcat-rename-lab\R07-folder-rename` | egész `Legacy/` mappa (25 fájl) → `Modern/`, 2 fájl szerkesztve | *Stage all changes* | 25 rename-sor vagy összevonás? a 2 szerkesztett (95%, 83%) is rename? |
| R08<br>`C:\tmp\gitcat-rename-lab\R08-staged-pure` | `git mv` stagelve, semmi más | *Unstage* a fájlon | staged rename megjelenése; unstage után a WIP-ben rename marad vagy szétesik? |
| R09<br>`C:\tmp\gitcat-rename-lab\R09-staged-plus-edit` | `git mv` stagelve + plusz **unstaged** szerkesztés az új fájlban | kattints a staged sorra, majd az unstaged sorra (két kép) | ugyanaz a fájl két szekcióban: staged = rename, unstaged = módosítás? |
| R10<br>`C:\tmp\gitcat-rename-lab\R10-half-del-staged` | törlés stagelve (`git rm`), új fájl még untracked | *Stage* az új fájlon | félig stagelt pár: staged oldalon törlés, unstaged oldalon új fájl, vagy rename? |
| R11<br>`C:\tmp\gitcat-rename-lab\R11-half-add-staged` | új fájl stagelve (`git add`), régi csak a lemezről törölve | *Stage* a törölt fájlon | fordított félpár |
| R12<br>`C:\tmp\gitcat-rename-lab\R12-ambiguous` | `source.txt` törölve, `copy-exact.txt` (azonos) és `copy-near.txt` (+1 sor) hozzáadva | – | melyiket párosítja (Git: az azonosat, a másik új fájl) |
| R13<br>`C:\tmp\gitcat-rename-lab\R13-duplicates` | két azonos tartalmú fájl (`first`, `second`) → `alpha`, `beta` | – | párosítás sorrendje (Git: first→alpha, second→beta) |
| R14<br>`C:\tmp\gitcat-rename-lab\R14-binary` | `logo.png` → `brand.png` (azonos), `icon.png` → `app-icon.png` (16 bájt átírva) | kattints mindkettőre | bináris rename; a módosított bináris is rename? kép-összehasonlító nézet? |
| R15<br>`C:\tmp\gitcat-rename-lab\R15-tiny-empty` | üres `old/.gitkeep` → `new/.gitkeep`; 2 soros `short.txt` → `brief.txt` 1 sor változással | – | üres és nagyon kicsi fájl párosul-e (Git: `.gitkeep` igen, `brief.txt` nem) |
| R16<br>`C:\tmp\gitcat-rename-lab\R16-case-only` | `Readme.md` → `README.md` stagelve (`git mv`); `Notes.md` → `notes.md` csak a lemezen | – | csak kis-nagybetűs rename; a csak lemezen átnevezett `notes.md` látszik-e egyáltalán (Git: nem) |
| R17<br>`C:\tmp\gitcat-rename-lab\R17-crlf` | rename + teljes fájl LF → CRLF | kattints a fájlra | sorvég-váltás + rename: rename marad? a diff minden sort változottnak mutat? |
| R18<br>`C:\tmp\gitcat-rename-lab\R18-swap` | `a.txt` és `b.txt` tartalma felcserélve (mindkét út létezik) | – | két módosítás, vagy „felcserélt” rename-ként érzékeli |
| R19<br>`C:\tmp\gitcat-rename-lab\R19-rename-and-modify-other` | rename + mellette sima módosított, törölt és új fájl | – | rendezés: hova kerül a rename-sor a listában (név szerint a régi vagy az új név alapján) |
| R20<br>`C:\tmp\gitcat-rename-lab\R20-sort-order` | `Zulu.cs` → `Alpha.cs`, mellette `Mike.cs` módosítva | – | rendezés egyértelműen: `Alpha` előre kerül (új név) vagy `Mike` után (régi név)? |

## H – commit history

| ID | Kiinduló állapot | Teendő | Fő kérdés |
| --- | --- | --- | --- |
| H01<br>`C:\tmp\gitcat-rename-lab\H01-history` | tiszta worktree, 6 commit, mindegyik egy rename-fajta | kattints sorban mind a 6 `rename: …` commitra, mindegyiknél kép a jobb panel fájllistájáról, és a fájlra kattintva a diffről | commit nézetben ugyanazok a szabályok? (Git: pure, 70%, mappa, case, bináris = rename; 45% = törlés + új) |

## S – stash

| ID | Kiinduló állapot | Teendő | Fő kérdés |
| --- | --- | --- | --- |
| S01<br>`C:\tmp\gitcat-rename-lab\S01-stash` | 1 stash: stagelt rename + szerkesztés, tiszta worktree | **a)** kattints a stash-re a gráfon, kép a fájllistáról és a diffről. **b)** *Apply stash* → kép a WIP panelről (staged vagy unstaged oldalra kerül a rename?) | stash fájllista rename-ként mutatja? apply után megmarad-e stagelt rename-nek |

## Prioritás, ha kevés az idő

R01 (a, b, c), R02 (a), R04 + R05, R09, R10, H01. Ezekből már a küszöb, a
stage/discard viselkedés és a diffnézet is eldönthető; a többi a széleket fedi.
