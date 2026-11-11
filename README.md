# DiskSize

**Finden Sie heraus, wo Ihr Speicherplatz geblieben ist.**

DiskSize ist ein schlankes Desktop-Programm, das ein Laufwerk einliest und alle
Ordner samt Unterordnern nach Größe sortiert anzeigt. So sehen Sie auf einen
Blick, welche Ordner den meisten Platz belegen – und können die Übersicht als
Excel-Datei exportieren.

- Läuft unter Windows, Linux und macOS
- Keine Installation, keine Cloud, keine Telemetrie
- Ändert keine Dateien – DiskSize liest nur

## Download und Schnellstart

Die aktuellen Builds stehen auf der
[Releases-Seite](https://github.com/wiederda/DiskSize/releases) bereit. Eine
Installation ist nicht nötig: Paket entpacken und `DiskSize.exe` starten.

1. **DiskSize starten.** Unter Windows genügt ein Doppelklick auf `DiskSize.exe`.
2. **Laufwerk auswählen.** Klicken Sie in der Liste auf das Laufwerk, das Sie
   untersuchen möchten. Der Scan startet sofort.
3. **Ergebnis ansehen.** Links sehen Sie den Ordnerbaum, rechts die Unterordner
   des ausgewählten Ordners mit Größe, Anteil und Größenbalken.
4. **Excel-Export (optional).** Sobald der Scan fertig ist, können Sie die
   komplette Ordnerübersicht als `.xlsx`-Datei speichern.

Der Scan läuft im Hintergrund, das Fenster bleibt währenddessen bedienbar. Je
nach Laufwerksgröße und Festplattengeschwindigkeit dauert er einige Sekunden
bis mehrere Minuten.

## Die Oberfläche

### Laufwerksauswahl

Beim Start zeigt DiskSize alle erkannten Laufwerke mit Gesamtgröße, belegtem und
freiem Speicher sowie einem Auslastungsbalken. Auch verbundene Netzlaufwerke mit
Laufwerksbuchstaben (z. B. `Z:\`) erscheinen in der Liste; nicht verbundene
Netzlaufwerke werden ausgeblendet.

- **Aktualisieren** liest die Laufwerksliste neu ein, z. B. nachdem ein
  USB-Stick angeschlossen wurde.
- Das Sonnen-/Mond-Symbol oben rechts wechselt zwischen hellem und dunklem
  Design. Die Auswahl wird gespeichert.

### Scan-Ansicht

Die Kopfzeile enthält den Pfad des untersuchten Laufwerks sowie diese
Schaltflächen:

| Schaltfläche | Funktion |
| --- | --- |
| **← Zurück** | Zurück zur Laufwerksauswahl. Ein laufender Scan wird abgebrochen. |
| **Erneut scannen** | Startet den Scan des Laufwerks neu. |
| **Excel-Export** | Speichert die Ordnerübersicht als `.xlsx`-Datei. Erst nach Abschluss des Scans verfügbar. |

Darunter fasst eine Leiste die wichtigsten Zahlen zusammen:

| Anzeige | Bedeutung |
| --- | --- |
| **Größe** | Gesamtgröße aller gefundenen Dateien im gescannten Laufwerk |
| **Belegt** | Vom Betriebssystem gemeldeter belegter Speicher |
| **Frei** | Vom Betriebssystem gemeldeter freier Speicher |
| **Dateien** | Anzahl der gefundenen Dateien |
| **Ordner** | Anzahl der gefundenen Ordner |
| **Zugriff verweigert** | Anzahl der Ordner, die nicht gelesen werden konnten |

Rechts daneben zeigt eine Fortschrittsanzeige den Stand des Scans an.

**Linke Seite – Ordnerbaum**

- Ein Klick auf einen Ordner wählt ihn aus und zeigt seinen Inhalt rechts an.
- Ein Klick auf das Dreieck (oder ein Doppelklick) klappt den Ordner auf bzw. zu.
- Die Zahl rechts neben dem Namen ist die Gesamtgröße inklusive aller
  Unterordner.

**Rechte Seite – Details**

Der Kopf zeigt Name, Pfad und Gesamtgröße des ausgewählten Ordners sowie seinen
Anteil am gesamten Laufwerk. **Im Explorer öffnen** zeigt den Ordner im
Dateimanager an (Windows: Explorer, macOS: Finder, Linux: Standard-Dateimanager).

Die Tabelle darunter listet die Unterordner auf:

| Spalte | Bedeutung |
| --- | --- |
| **Name** | Name des Unterordners |
| **Größe** | Gesamtgröße inklusive aller Unterordner |
| **Anteil** | Anteil am ausgewählten Ordner |
| **Elemente** | Anzahl der direkten Unterordner |
| **Größenverteilung** | Balken zur schnellen visuellen Einordnung |

Ein Klick auf eine Zeile öffnet den Ordner und zeigt wiederum dessen Inhalt.

## Excel-Export

Der Export ist verfügbar, sobald der Scan abgeschlossen ist. Beim Klick öffnet
sich ein Speichern-Dialog; als Dateiname wird `DiskSize_<Laufwerk>.xlsx`
vorgeschlagen.

Die Arbeitsmappe enthält das Blatt **Ordner** mit einer Kopfzeile (Quelle,
Gesamtgröße, Anzahl Ordner) und diesen Spalten:

- Ebene
- Name
- Pfad
- Größe (Bytes)
- Größe (lesbar formatiert)
- Anteil an der Gesamtgröße
- Status (z. B. „Zugriff verweigert“)

Der Export enthält **nur Ordner, keine einzelnen Dateien**. Bei extrem großen
Laufwerken wird der Export am Zeilenlimit von Excel (1.048.576 Zeilen)
abgeschnitten.

## Zugriffsrechte

Einige Ordner – etwa `System Volume Information` oder die Profile anderer
Benutzer – dürfen nur mit Administratorrechten gelesen werden.

- Nicht lesbare Ordner werden **rot und kursiv** dargestellt. In der
  Zusammenfassung und im Excel-Export erscheint der Hinweis
  „Zugriff verweigert“.
- Ihre Größe zählt nicht mit; die Zusammenfassung zeigt an, wie viele Ordner
  betroffen waren.
- Für ein möglichst vollständiges Ergebnis starten Sie DiskSize unter Windows
  über einen Rechtsklick auf die EXE und **Als Administrator ausführen**.

Für normale Benutzerordner sind keine besonderen Rechte nötig.

## Gut zu wissen

- **Es wird immer ein ganzes Laufwerk gescannt.** Ein einzelner Ordner kann
  derzeit nicht als Startpunkt gewählt werden.
- **Verknüpfungen werden nicht verfolgt.** Symlinks und Junctions werden
  übersprungen, damit keine doppelten Größen entstehen und keine Endlosschleifen
  auftreten.
- **Die Summen können leicht abweichen.** „Belegt“ und „Frei“ stammen vom
  Betriebssystem und enthalten auch Dateisystem-Overhead, reservierte Bereiche
  oder Dateien, auf die nicht zugegriffen werden kann.
- **Dateien direkt im Stammverzeichnis** sind in der Gesamtgröße enthalten,
  erscheinen aber nicht als eigene Zeile in der Ordnertabelle.
- **Einstellungen** (helles/dunkles Design) werden unter
  `%APPDATA%\DiskSize\settings.txt` (Windows) bzw.
  `~/.config/DiskSize/settings.txt` (Linux/macOS) gespeichert.

## CLI-Version

Neben der grafischen Oberfläche gibt es `disksize-cli`. Sie scannt einen oder
mehrere Ordner und schreibt **pro Ordner ein eigenes Arbeitsblatt** in eine
Excel-Datei. Die CLI steht ebenfalls auf der
[Releases-Seite](https://github.com/wiederda/DiskSize/releases) zur Verfügung.

```powershell
disksize-cli "C:\Users" "D:\Projekte" -o Bericht.xlsx
```

Aufruf:

```
disksize-cli [OPTIONEN] <ORDNER>...
```

| Option | Bedeutung |
| --- | --- |
| `-o`, `--output <DATEI>` | Zieldatei. Ohne Angabe: `DiskSize_<Ordner>.xlsx` bei einem Ordner, sonst `DiskSize_Export.xlsx`. |
| `-q`, `--quiet` | Keine Statusmeldungen ausgeben. |
| `-h`, `--help` | Hilfe anzeigen. |
| `-V`, `--version` | Version anzeigen. |

Beispiele:

```powershell
# Ein Ordner
disksize-cli "C:\Users"

# Mehrere Ordner in eine Arbeitsmappe, ein Blatt pro Ordner
disksize-cli "C:\Users" "D:\Projekte" -o Bericht.xlsx
```

Die Blattnamen werden aus den Ordnernamen gebildet. Ungültige Excel-Zeichen
werden ersetzt, die Namen auf 31 Zeichen gekürzt und bei Doppelungen mit
`(2)`, `(3)` usw. ergänzt. Fortschritt und Zusammenfassung erscheinen auf
stderr; die Excel-Datei wird erst geschrieben, nachdem alle angegebenen Ordner
gescannt wurden..

