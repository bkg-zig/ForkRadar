# Fork-Netzwerk: belegte Erfassung

Diese Datei beschreibt eine echte, reproduzierbare Erfassung zweier Fork-Netzwerke
über die öffentliche GitHub-REST-API. Sie entstand im Auftrag des Agents
"Fork Graph Researcher" im Team "ForkRadar Agent Team" am 2026-10-30.

Die Rohdaten liegen unter `docs/evidence/fork-network/`. Die Erfassung nutzt nur
Leseendpunkte der API und keinen API-Key.

## Ergebnis in Kürze

| Größe | chmln/sd | Semmle/SecurityExploits |
| --- | --- | --- |
| GitHub-Id | 162863623 | 137218373 |
| Ist selbst ein Fork | nein | nein |
| Archiviert | nein | ja |
| Standardbranch | master | master |
| Vom Repo gemeldete `forks_count` | 175 | 50 |
| Vom Fork-Listing gelieferte direkte Kinder | 223 | 54 |
| Erreichte Tiefe | 2 | 2 |
| Releases | 12 | 0 |
| Erfasste Commits (neueste Seite) | 100 | 58 |
| Neuester Commit | 2026-02-25T19:25:13Z | 2019-08-16T01:27:18Z |
| Vom Listing gelistete, aber nicht mehr auflösbare Forks | 2 | 2 |
| Ausgeführte Upstream-Vergleiche | 4 | 4 |
| Laufstatus | PARTIAL | PARTIAL |

Nur die vom Listing gelieferten Knoten zählen als "belegter Fork". Die Zahl 223
beziehungsweise 54 ist die Menge disjunkter Repository-Ids aus dem Listing, nicht
die gemeldete `forks_count`. Beide Zahlen weichen voneinander ab, dazu unten.

## Autorisierte Quelle

Quelle ist `https://api.github.com` (GitHub REST API, Version `2022-11-28`,
Header `Accept: application/vnd.github+json`). Der Lauf war unauthentifiziert.
Ein unauthentifizierter Aufruf unterliegt 60 Anfragen je Stunde, der Wert stand
in `x-ratelimit-limit`. Der Harness liest den Reststand aus den Antwortheadern
und pausiert bei Erreichen von 0 bis zum dokumentierten Reset-Zeitpunkt, statt
Arbeit zu verlieren.

Verwendete Endpunkte:

```text
GET /repos/{owner}/{repo}
GET /repos/{owner}/{repo}/releases?per_page=100
GET /repos/{owner}/{repo}/commits?sha={branch}&per_page=100
GET /repos/{owner}/{repo}/forks?per_page=100&page={n}
GET /repos/{owner}/{repo}/compare/{base}...{head}
```

## Methode

1. Wurzelrepository lesen, Releases und die neueste Commit-Seite des Standardbranchs
   erfassen.
2. Fork-Listing der Wurzel vollständig durchblättern.
3. Jeden Fork, dessen eigene `forks_count` über 0 liegt, in eine Warteschlange
   legen und sein Listing ebenfalls durchblättern. Die Tiefe eines Knotens ist
   seine Entfernung in dieser Breitensuche.
4. Aus der Menge der Forks eine Stichprobe ziehen (bevorzugt Forks mit eigenen
   Forks, danach die zuletzt gepushten) und für jeden Knoten `GET /repos/{full}`
   aufrufen. Dieser Aufruf liefert den gemeldeten Parent und Source und klärt, ob
   das Repository noch existiert.
5. Jeden lebenden Stichprobenknoten gegen seinen gemeldeten Upstream vergleichen
   und `status`, `ahead_by`, `behind_by` und `total_commits` festhalten.

Der Lauf berichtet `COMPLETED` nur, wenn jede Seite des Umfangs gelesen wurde.
Beide Läufe melden `PARTIAL`, weil einzelne Listing-Einträge nicht mehr auflösbar
waren. Das ist ein belegter Befund und kein Abbruch des Laufs.

## Nachweis der Rekursion (Tiefe 2)

Ein Knoten der Tiefe 2 liegt nicht im Listing der Wurzel. Er ist nur über das
Listing seines eigenen Parents erreichbar. Vier solcher Kanten wurden erfasst:

```text
chmln/sd                -> VonRosenchild/sd      -> HubBucket-Team/sd
Semmle/SecurityExploits -> Mdlglobal-atlassian-net/SecurityExploits -> Mshaffar/SecurityExploits
Semmle/SecurityExploits -> awesome-security/SecurityExploits        -> ProjectZeroDays/SecurityExploits
Semmle/SecurityExploits -> ThomasKing2014/SecurityExploits          -> hohjgdhsj/SecurityExploits
```

Beleg: die jeweilige Antwort von `GET /repos/{parent}/forks?per_page=100&page=1`
im Rohdatenordner. Für Mshaffar kam die Bestätigung zusätzlich aus
`GET /repos/Mshaffar/SecurityExploits` mit `parent = Mdlglobal-atlassian-net/SecurityExploits`
und `source = Semmle/SecurityExploits`.

Damit ist belegt, dass eine vollständige Abdeckung die Rekursion braucht. Das
Listing der Wurzel allein liefert diese Knoten nicht.

## Upstream-Vergleich

Verglichen wurde jeder lebende Stichprobenknoten gegen den Parent, den GitHub
selbst meldet. Basisbranch war der Standardbranch des Parents, Kopfbranch der
Standardbranch des Forks.

chmln/sd:

| Fork | Status | ahead_by | behind_by |
| --- | --- | --- | --- |
| VonRosenchild/sd | behind | 0 | 96 |
| simono/sd | identical | 0 | 0 |
| uzumakienes/sd | identical | 0 | 0 |
| pepa65/esed | diverged | 71 | 15 |

Semmle/SecurityExploits:

| Fork | Status | ahead_by | behind_by |
| --- | --- | --- | --- |
| Mdlglobal-atlassian-net/SecurityExploits | identical | 0 | 0 |
| awesome-security/SecurityExploits | behind | 0 | 2 |
| ThomasKing2014/SecurityExploits | behind | 0 | 26 |
| AmesianX/SecurityExploits | identical | 0 | 0 |

`pepa65/esed` ist ein umbenannter Fork von `chmln/sd`: `name = esed`,
`parent = chmln/sd`. Er weicht am stärksten ab und ist der einzige divergierte
Knoten der Stichprobe. Das belegt, dass ein Fork anhand der numerischen Id
zugeordnet werden muss, nicht anhand des Namens.

Ein Vergleich liefert `status = identical` auch dann, wenn der Fork keinen
eigenen Commit trägt. `identical` bedeutet damit "kein eigener Commit gegenüber
dem Upstream", nicht "in Benutzung".

## Beobachtetes API-Verhalten und Folgen für den Code

Diese Punkte sind gemessen, nicht abgeleitet. Jeder Punkt nennt den Rohbeleg.

1. Das Fork-Listing trägt weder `parent` noch `source`. Die Felder fehlen im
   JSON-Objekt vollständig, sie sind nicht null gesetzt. Geprüft über alle acht
   Listing-Antworten des Laufs, kein einziger Eintrag trug die Schlüssel.
   Folge: `GitHubClient::list_forks` in `crates/forkradar-github/src/lib.rs`
   erzeugt Knoten mit `parent = None` und `source = None`. Die in
   `crates/forkradar-store/src/scans.rs` ab Zeile 330 vorgesehenen
   `GITHUB_REPORTED_PARENT`, `GITHUB_REPORTED_SOURCE` und `FORK_NETWORK_MEMBER`
   Kanten können aus einem Listing allein nie entstehen. Wer sie braucht, ruft
   `GET /repos/{owner}/{name}` je Knoten ab.

2. Das Fork-Listing liefert mehr Einträge als `forks_count`. chmln/sd meldet 175
   und liefert 223 disjunkte Kinder über drei Seiten. Semmle/SecurityExploits
   meldet 50 und liefert 54 über eine Seite. Folge: die Schleife darf nicht über
   `forks_count` gesteuert werden. Die Engine blättert korrekt, bis eine Seite
   kürzer als `per_page` ist beziehungsweise der Link-Header endet.

3. Das Fork-Listing kann Einträge enthalten, deren Repository nicht mehr
   auflösbar ist. chmln/sd listet `vkezcbrdsg232272/sd` und `linyu21q/sd`,
   beide mit `forks_count = 1`; `GET /repos/{full}` antwortet für beide mit
   HTTP 404. Semmle/SecurityExploits listet `Vacryptogodfather45/SecurityExploits`
   und `isabella232/SecurityExploits`, beide ebenso 404. Folge: die Engine legt
   den Knoten aus dem Listing an und reiht einen Job ein; der Job endet später
   mit `REPOSITORY_NOT_FOUND`. Der Knoten bleibt ohne Lebendigkeitsmerkmal
   gespeichert. Ein `liveness`-Feld oder ein eigener Status wäre nötig, damit die
   Oberfläche keinen toten Fork als aktiven Knoten zeigt.

4. `GET /repos/{base}/compare/{base_branch}...{head}` antwortet mit HTTP 404,
   wenn der Kopf nicht auflösbar ist. In beiden Netzwerken traf das die
   nicht mehr auflösbaren Forks. Folge: der Vergleich muss 404 als "nicht
   verfügbar" behandeln, nicht als wiederholbaren Fehler.

5. Ein Fork kann umbenannt sein. `pepa65/esed` gehört zum Netzwerk von
   `chmln/sd`. Folge: Zuordnung über `id`, nicht über `full_name`.

6. `forks_count` kann zwischen zwei Aufrufen abweichen. `Semmle/SecurityExploits`
   meldete im ersten Aufruf 50, das Listing lieferte 54 Einträge. Das ist ein
   gemessener Unterschied. Eine Ursache ist mit diesen Daten nicht belegt.

## Grenzen dieser Erfassung

- Beide Netzwerke sind vollständig durchblättert, aber nicht vollständig gegen
  die API verifiziert. Nur 5 Knoten je Netzwerk haben einen Detailaufruf
  erhalten, deshalb steht für 218 beziehungsweise 51 Knoten `liveness = UNKNOWN`.
- Die Commit-Erfassung liest die neueste Seite des Standardbranchs
  (`per_page=100`). Sie ist keine vollständige Historie. Für chmln/sd reicht die
  Seite vom 2026-02-25 bis 2021-05-06, ältere Commits wurden nicht geholt.
- Releases wurden nur für die Wurzel erfasst, nicht je Fork.
- Forks ohne eigene Forks werden nicht weiter rekursiert, weil ihre Tiefe nicht
  wächst. Ein Fork mit `forks_count = 0` kann dennoch gelöschte Enkel haben, die
  das Listing nicht mehr führt.
- Der Lauf war unauthentifiziert. Mit einem Token stünden 5000 Anfragen je Stunde
  zur Verfügung, und die Stichproben könnten deutlich größer werden.

## Reproduktion

```text
python3 docs/evidence/fork-network/harvest.py \
  --out /tmp/forkradar-capture \
  --target chmln/sd \
  --target Semmle/SecurityExploits \
  --sample-size 6 --compare-size 6
```

Der Lauf schreibt je Endpunkt eine Rohantwort und eine `graph.json` in den
Ausgabeordner. Ein zweiter Lauf nutzt vorhandene Rohantworten und gibt nur für
fehlende Pfade neue Anfragen aus.

## Rohdaten

`docs/evidence/fork-network/capture-2026-10-30.json` enthält das
zusammengefasste Ergebnis: Knoten mit Tiefe, Parent, Source, Lebendigkeit und
Zeitstempeln, die Kanten, die Releases der Wurzel, den Kopf der Commit-Liste und
alle acht Upstream-Vergleiche. Die vollständigen Rohantworten der API liegen
außerhalb des Repositories im Laufverzeichnis und sind über den Harness
reproduzierbar.
