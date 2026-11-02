# FrigatePiP — Benutzerhandbuch

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP ist eine kleine Windows-Anwendung im Infobereich der Taskleiste für [Frigate NVR](https://frigate.video). Erkennt Frigate eine Person, ein Auto oder ein anderes Objekt, zeigt FrigatePiP sofort in einer Bildschirmecke ein kleines Bild-in-Bild-Fenster mit dem Livevideo dieser Kamera. Bewegt sich nichts mehr, schließt sich das Fenster von selbst.

## Inhalt

1. [Voraussetzungen](#1-voraussetzungen)
2. [Installation](#2-installation)
3. [Erster Start](#3-erster-start)
4. [Das Tray-Symbol](#4-das-tray-symbol)
5. [Vorschaufenster](#5-vorschaufenster)
6. [Das Einstellungsfenster](#6-das-einstellungsfenster)
7. [Die Einstellungsdatei](#7-die-einstellungsdatei)
8. [Befehlszeile](#8-befehlszeile)
9. [Sprache](#9-sprache)
10. [Dateien, Datenschutz und Ressourcen](#10-dateien-datenschutz-und-ressourcen)
11. [Fehlerbehebung](#11-fehlerbehebung)

---

## 1. Voraussetzungen

- **Windows 10 (Version 1809 oder neuer) oder Windows 11**, 64 Bit.
- **Frigate** muss vom PC aus über das Netzwerk erreichbar sein. Empfohlen wird Frigate 0.14 oder neuer; die *Review*-Modi erfordern 0.14 oder neuer.
- Der **API-Port 5000** von Frigate muss vom PC aus erreichbar sein. FrigatePiP meldet sich nicht an, daher kann der authentifizierte Port 8971 nicht verwendet werden. Veröffentlichen Sie den Port unter Docker, z. B. `- "5000:5000"`.
- **Frigate-Add-on für Home Assistant:** Port 5000 ist standardmäßig deaktiviert – aktivieren Sie ihn in den *Netzwerk*-Einstellungen des Add-ons. HTTPS-Adressen werden nicht unterstützt: Verwenden Sie die normale `http://`-Adresse im lokalen Netzwerk.
- Für flüssiges Livevideo: der in Frigate integrierte **go2rtc-RTSP-Server, Port 8554** (`- "8554:8554"`). Ohne ihn können Sie weiterhin MJPEG oder Schnappschüsse verwenden (siehe [Livebild](#livebild)).
- Optional: ein von Frigate verwendeter **MQTT-Broker**, falls Sie MQTT dem Frigate-WebSocket vorziehen.
- Für H.265-Kameras (HEVC) benötigt Windows einen HEVC-Decoder. Die meisten PCs mit aktuellem Grafiktreiber haben einen; andernfalls installieren Sie die *HEVC-Videoerweiterungen* aus dem Microsoft Store oder verwenden einen H.264-Sub-Stream. Ist kein Decoder verfügbar, zeigt FrigatePiP automatisch stattdessen MJPEG von Frigate.
- **Windows-N-Editionen** (z. B. Windows 11 Pro N) enthalten Media Foundation erst, wenn Sie das optionale Windows-Feature *Media Feature Pack* hinzufügen. FrigatePiP funktioniert auch ohne, das Livebild kommt dann jedoch als MJPEG von Frigate (ca. 5 Bilder pro Sekunde), und AAC-Kameraton ist nicht verfügbar.

## 2. Installation

### Mit dem Installationsprogramm (empfohlen)

1. Laden Sie `FrigatePiP-Setup-<version>.exe` von der Seite [Releases](https://github.com/maciejgornicz/frigate-pip/releases) herunter.
2. Führen Sie die Datei aus. Das Installationsprogramm benötigt keine Administratorrechte; es installiert nur für den aktuellen Benutzer, nach `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Wählen Sie, ob FrigatePiP **mit Windows starten** soll (standardmäßig aktiviert) und ob eine Desktopverknüpfung erstellt werden soll.
4. Lassen Sie am Ende **FrigatePiP starten** aktiviert. Das Tray-Symbol erscheint, und beim ersten Start öffnet sich das Einstellungsfenster.

Installationsprogramm und Anwendung sind nicht mit einem Code-Signing-Zertifikat signiert. Windows SmartScreen zeigt möglicherweise *„Der Computer wurde durch Windows geschützt“* an; klicken Sie auf **Weitere Informationen → Trotzdem ausführen**. Sie können die mit jedem Release veröffentlichte SHA-256-Prüfsumme kontrollieren.

### Portabel (ohne Installation)

Laden Sie `frigate-pip.exe` von der Releases-Seite herunter und starten Sie die Datei aus einem beliebigen Ordner. Die Einstellungen werden am selben Ort gespeichert wie bei der Installation (siehe [Dateien](#10-dateien-datenschutz-und-ressourcen)). Verwenden Sie in den Einstellungen **FrigatePiP mit Windows starten**, um es automatisch zu starten.

### Aktualisieren

Führen Sie das Installationsprogramm der neuen Version aus. Es beendet das laufende FrigatePiP, ersetzt das Programm und behält Ihre Einstellungen bei. Bei der portablen Version beenden Sie FrigatePiP und ersetzen die EXE-Datei.

### Deinstallieren

**Einstellungen → Apps → Installierte Apps → FrigatePiP → Deinstallieren** (oder *Programme hinzufügen oder entfernen*). Das Deinstallationsprogramm beendet FrigatePiP und entfernt das Programm, die Verknüpfungen und den Autostart-Eintrag. Ihre Einstellungen und das Protokoll in `%APPDATA%\FrigatePiP` bleiben erhalten; löschen Sie diesen Ordner, um auch sie zu entfernen.

## 3. Erster Start

1. Beim ersten Start öffnet sich das Einstellungsfenster auf der Registerkarte **Verbindung**.
2. Geben Sie die **Frigate-Adresse** ein, z. B. `http://192.168.1.10:5000`.
3. Klicken Sie auf **Verbindung testen**. FrigatePiP prüft, ohne etwas zu speichern:
   - die Frigate-API und ihre Version,
   - die Kameraliste,
   - die Ereignisquelle (WebSocket oder MQTT),
   - das RTSP-Video von go2rtc, einschließlich Codec und ob die Kamera Audio hat.

   Jede Zeile beginnt mit ✔ (funktioniert) oder ✖ (Problem, mit Hinweis).
4. Klicken Sie auf **OK**. Ab jetzt wartet FrigatePiP im Hintergrund auf Erkennungen.
5. Um zu sehen, wie ein Fenster aussieht, klicken Sie mit der rechten Maustaste auf das Tray-Symbol und wählen **Kamera anzeigen → (eine Kamera)**, oder aktivieren Sie **Live anzeigen** unten im Einstellungsfenster.

## 4. Das Tray-Symbol

Das Symbol im Infobereich der Taskleiste zeigt den Status:

| Farbe | Bedeutung |
|---|---|
| Blau | Mit Frigate verbunden, wartet auf Erkennungen |
| Grau | Nicht verbunden (FrigatePiP versucht es automatisch weiter) |
| Orange | Benachrichtigungen sind pausiert |

Zeigen Sie mit der Maus auf das Symbol, um den Status zu sehen (z. B. *Verbunden: http://192.168.1.10:5000*).

- **Linksklick** öffnet das Einstellungsfenster.
- **Rechtsklick** öffnet das Menü:

| Menüeintrag | Funktion |
|---|---|
| *(Statuszeile)* | Verbindungsstatus oder verbleibende Pausenzeit (nur zur Information). |
| **Einstellungen…** | Öffnet das Einstellungsfenster. |
| **Kamera anzeigen** | Öffnet sofort ein Vorschaufenster der gewählten Kamera, auch während Benachrichtigungen pausiert sind. Nützlich, um das Bild zu prüfen oder einfach nachzusehen. |
| **Benachrichtigungen pausieren** | **Für 15 Minuten**, **Für 1 Stunde** oder **Bis zur Fortsetzung**. Während der Pause öffnen Erkennungen keine Fenster. Bei aktiver Pause lautet der Eintrag **Pausiert**, und **Jetzt fortsetzen** beendet sie. |
| **Vorschaufenster schließen** | Schließt alle geöffneten Vorschaufenster, auch angeheftete. |
| **Frigate öffnen** | Öffnet die Frigate-Weboberfläche in Ihrem Browser. |
| **Hilfe** | Öffnet dieses Handbuch. |
| **Protokoll anzeigen** | Öffnet die Protokolldatei im Editor (nützlich, wenn etwas nicht funktioniert). |
| **Einstellungsdatei (erweitert)…** | Öffnet `config.ini` im Editor. Gespeicherte Änderungen werden sofort wirksam. |
| **Beenden** | Beendet FrigatePiP. |

## 5. Vorschaufenster

Wenn eine Erkennung Ihren Einstellungen entspricht, erscheint in der gewählten Bildschirmecke ein Vorschaufenster:

1. Das Fenster erscheint sofort, wenige Millisekunden nach der Meldung von Frigate.
2. Es zeigt zuerst den neuesten Schnappschuss der Kamera aus Frigate und wechselt dann zum Livevideo, sobald der Stream ein Bild liefert (meist innerhalb einer Sekunde, mit dem [Vorpuffer](#vorpuffer) nahezu sofort).
3. Die Beschriftung (falls aktiviert) zeigt den Kameranamen, das Objekt (z. B. *Person*), das Sub-Label (z. B. ein erkanntes Gesicht oder Kennzeichen), die Erkennungssicherheit und die Uhrzeit. Ein **roter Punkt** bedeutet, dass das Bild live ist.

Vorschaufenster übernehmen nie den Tastaturfokus und unterbrechen daher weder das Tippen noch Spiele.

### Mausaktionen

| Aktion | Wirkung |
|---|---|
| **Ziehen** | Verschiebt das Fenster. Es bleibt dort, wo Sie es abgelegt haben, bis es sich schließt. |
| **Klicken** | Das Fenster wird **angeheftet**: Es bleibt geöffnet, bis Sie es schließen (die Beschriftung zeigt *angeheftet*). Erneut klicken, um es zu lösen. |
| **Mausrad** | Vergrößert oder verkleinert das Fenster. |
| **×** (oben rechts) | Schließt das Fenster sofort. Die Schaltfläche erscheint, wenn Sie mit der Maus auf das Fenster zeigen. |
| **Rechtsklick** | Schließt das Fenster. |
| **Doppelklick** | Öffnet diese Kamera in der Frigate-Weboberfläche. |
| **Darauf zeigen** | Solange sich der Mauszeiger über dem Fenster befindet, schließt es sich nicht. Nachdem Sie die Maus wegbewegt haben, bleibt es noch mindestens 3 Sekunden geöffnet. |

### Wann schließt sich ein Fenster?

- Nach der unter **Dauer nach letzter Bewegung** eingestellten Zahl von Sekunden, nachdem sich das Objekt (oder ein anderes passendes Objekt dieser Kamera) zuletzt bewegt hat.
- Spätestens nach der unter **höchstens** eingestellten Zahl von Sekunden, auch wenn sich das Objekt weiter bewegt.
- Nie, solange es angeheftet ist oder sich der Mauszeiger darüber befindet.

### Mehrere Kameras

Jede Kamera hat höchstens ein Fenster. Neue Erkennungen einer Kamera, die bereits ein Fenster hat, verlängern nur dessen Anzeige und aktualisieren die Beschriftung. Fenster verschiedener Kameras werden von der gewählten Ecke aus gestapelt. **Max. Anzahl Fenster** begrenzt, wie viele gleichzeitig geöffnet sind; **Wenn alle belegt sind** legt fest, ob eine neue Kamera das älteste Fenster ersetzt (an derselben Stelle auf dem Bildschirm; angeheftete Fenster werden nie ersetzt) oder übersprungen wird.

## 6. Das Einstellungsfenster

Öffnen Sie es per Linksklick auf das Tray-Symbol oder mit **Einstellungen…** im Menü. **Jede Option hat eine Beschreibung: Zeigen Sie mit der Maus darauf.** Drücken Sie **F1** oder klicken Sie auf **Hilfe**, um dieses Handbuch zu öffnen.

Die meisten Änderungen wirken sich **sofort** auf geöffnete Vorschaufenster aus, sodass Sie das Ergebnis vor dem Speichern sehen. **OK** speichert und schließt, **Übernehmen** speichert und lässt das Fenster geöffnet, **Abbrechen** (oder das Schließen des Fensters) verwirft nicht gespeicherte Änderungen.

### 6.1 Verbindung

| Option | Beschreibung |
|---|---|
| **Frigate-Adresse** | Adresse des Frigate-Servers mit Port, z. B. `http://192.168.1.10:5000` oder `http://frigate.local:5000`. Port 5000 erfordert keine Anmeldung. |
| **Ereignisquelle** | **Frigate-WebSocket (empfohlen)** — FrigatePiP verbindet sich direkt mit Frigate; sonst wird nichts benötigt. **MQTT** — Erkennungen werden von Ihrem MQTT-Broker gelesen (dem in Frigate konfigurierten). Beide liefern dasselbe Ergebnis; der WebSocket ist meist etwas schneller. |
| **MQTT-Server**, **Port** | Hostname oder IP-Adresse und Port (meist 1883) des MQTT-Brokers. Wird nur mit der Quelle MQTT verwendet. |
| **Benutzer / Passwort** | MQTT-Anmeldedaten. Leer lassen, wenn der Broker anonymen Zugriff erlaubt. |
| **MQTT-Topic-Präfix** | `mqtt.topic_prefix` aus der Frigate-Konfiguration, meist `frigate`. |
| **RTSP-Adresse (go2rtc)** | Vorlage der Adresse des Livevideos. `{host}` wird durch den Host der Frigate-Adresse und `{stream}` durch den Namen des go2rtc-Streams ersetzt. Der Standardwert `rtsp://{host}:8554/{stream}` passt zu einer Standardinstallation von Frigate. Wenn go2rtc eine Anmeldung erfordert, verwenden Sie `rtsp://user:password@{host}:8554/{stream}`. |
| **Verbindung testen** | Prüft die oben eingegebenen Werte (Frigate, Kameras, Ereignisse, RTSP-Video und Audio), ohne sie zu speichern. |
| **Sprache** | Sprache von FrigatePiP. **Automatisch (Windows)** folgt der Windows-Anzeigesprache; ist diese Sprache nicht verfügbar, wird Englisch verwendet. Siehe [Sprache](#9-sprache). |
| **FrigatePiP mit Windows starten** | Startet FrigatePiP automatisch, wenn Sie sich bei Windows anmelden. |

### 6.2 Erkennung

| Option | Beschreibung |
|---|---|
| **Fenster anzeigen für** | Welche Frigate-Meldungen ein Fenster öffnen: **Jedes erkannte Objekt (am schnellsten)** — jedes von Frigate verfolgte Objekt; **Nur Review-Alarme** — nur Review-Einträge mit dem Schweregrad *alert*; **Review-Alarme und -Erkennungen** — alle Review-Einträge. Review-Einträge folgen den in der `review`-Konfiguration von Frigate festgelegten Regeln für Alarme und Erkennungen (Labels und erforderliche Zonen) und treffen etwas später ein als Objektereignisse. |
| **Kameras** | Kameras, die ein Fenster öffnen dürfen. Nicht ausgewählte Kameras werden ignoriert. Sind alle Kameras ausgewählt, werden auch später zu Frigate hinzugefügte Kameras berücksichtigt. |
| **Objekte** | Objekttypen (Labels), die ein Fenster öffnen dürfen, z. B. Person, Auto, Hund. Die Liste zeigt die von Ihren Kameras verfolgten Objekte. Sind alle ausgewählt, zählt jeder Objekttyp. |
| **Erforderliche Zonen** | Wenn Sie Zonen auswählen, öffnet sich ein Fenster nur, wenn das Objekt eine davon betritt. Ist keine ausgewählt, zählt jede Stelle im Bild. |
| **Unbewegte Objekte ignorieren** | Objekte, die Frigate als stationär markiert (z. B. ein geparktes Auto), öffnen keine Fenster und halten sie nicht offen. Gilt für *Jedes erkannte Objekt*. |
| **Min. Erkennungssicherheit** | Erkennungen mit geringerer Sicherheit werden ignoriert. 0 % bedeutet, dass die Schwellenwerte von Frigate entscheiden. Gilt für *Jedes erkannte Objekt*. |

Fehlalarme (False Positives) von Frigate werden immer ignoriert. Jedes Objekt öffnet nur einmal ein Fenster; bewegt es sich erneut, verlängert es nur das geöffnete Fenster.

### 6.3 Vorschaufenster

| Option | Beschreibung |
|---|---|
| **Monitor** | Monitor, auf dem die Fenster erscheinen: **Hauptmonitor** oder ein bestimmter Monitor (von links nummeriert, mit seiner Auflösung). |
| **Bildschirmecke** | Ecke, in der die Fenster erscheinen. Weitere Fenster werden von dieser Ecke aus gestapelt. |
| **Fensterbreite** | Breite in Pixeln (bei 100 % Skalierung; FrigatePiP passt sie an die Skalierung des Monitors an). Die Höhe ergibt sich aus dem Seitenverhältnis der Kamera. Die Größe eines einzelnen Fensters können Sie auch mit dem Mausrad ändern. |
| **Abstand zum Rand** | Abstand zwischen den Fenstern und den Bildschirmrändern (und zwischen den Fenstern). |
| **Dauer nach letzter Bewegung** | Wie viele Sekunden ein Fenster nach der letzten Bewegung geöffnet bleibt. |
| **höchstens** | Maximale Anzeigedauer eines Fensters in Sekunden, auch wenn sich das Objekt weiter bewegt (es sei denn, es ist angeheftet). |
| **Max. Anzahl Fenster** | Wie viele Fenster (Kameras) gleichzeitig geöffnet sein können (1–8). |
| **Wenn alle belegt sind** | **Ältestes Fenster ersetzen (gleiche Position)** — die neue Kamera nimmt den Platz des ältesten nicht angehefteten Fensters ein; **Neues Ereignis überspringen** — die neue Erkennung wird ignoriert. |
| **Immer im Vordergrund** | Hält die Fenster über allen anderen Fenstern. |
| **Beschriftung im Bild** | Zeigt Kamera, Objekt, Erkennungssicherheit und Uhrzeit im Bild an. |
| **Benachrichtigungston** | Spielt den Windows-Sound *Sternchen* ab, wenn ein neues Fenster erscheint. Dies ist nicht der Kameraton (siehe [Kameraton](#kameraton)). |
| **Während Spielen oder Präsentationen im Vollbild nicht anzeigen** | Es erscheinen keine Fenster, während ein Spiel, ein Video oder eine Präsentation im Vollbild läuft (Erkennungen werden übersprungen, nicht verzögert). |

### 6.4 Video & Audio

#### Livebild

| Auswahl | Beschreibung |
|---|---|
| **RTSP von go2rtc** (Standard) | Flüssiges Video mit der Bildrate der Kamera und Hardware-Decodierung (H.264 und H.265), geringe Verzögerung. Benötigt Port 8554. |
| **MJPEG von Frigate** | Ca. 5 Bilder pro Sekunde in der Erkennungsauflösung (detect), über Port 5000. Verwenden Sie diese Option, wenn RTSP nicht verfügbar ist. |
| **Nur aktualisierte Schnappschüsse** | Nur die Schnappschüsse von Frigate, regelmäßig aktualisiert. Die sparsamste Option. |

#### Stream

| Auswahl | Beschreibung |
|---|---|
| **Automatisch (nach Fenstergröße)** | Der ressourcenschonende **sub**-Stream für normale Fenster, der **main**-Stream in voller Auflösung, wenn das Fenster breiter ist als **Main ab Fensterbreite** (in physischen Pixeln). |
| **Sub (ressourcenschonend)** | Immer der Sub-Stream. |
| **Main (volle Auflösung)** | Immer der Main-Stream. |

FrigatePiP ermittelt die go2rtc-Streams jeder Kamera aus der Frigate-Konfiguration (`live.streams`, `live.stream_name` oder go2rtc-Streams namens `<camera>` und `<camera>_sub`). Sie können sie in der [Einstellungsdatei](#einstellungen-pro-kamera) auch selbst festlegen.

#### Kameraton

**Kameraton** gibt den Ton der Kamera (AAC oder G.711 aus dem RTSP-Stream) in jedem geöffneten Fenster wieder. Er läuft separat und verzögert daher nie das Fenster oder das Video. Die Lautstärke lässt sich im Lautstärkemixer von Windows einstellen (*FrigatePiP*). Erfordert **RTSP von go2rtc**.

#### Vorpuffer

**Vorpuffer** hält eine Verbindung zum Sub-Stream jeder Kamera, sodass das Livevideo sofort startet (ca. 0,15 s), statt auf das nächste Schlüsselbild zu warten (bis zu ca. 1 s, je nach Schlüsselbildintervall der Kamera). Kosten: ständig ca. 0,1–0,5 Mbit/s Datenverkehr im lokalen Netzwerk pro Kamera.

#### GPU bereithalten

**GPU bereithalten** hält das Grafikgerät initialisiert, sodass das Video ca. 130 ms früher startet. Kosten: ca. 1 % eines CPU-Kerns im Leerlauf (Thread des Grafiktreibers).

### 6.5 Unterer Fensterbereich

| Element | Beschreibung |
|---|---|
| **Vorschau:** Kamera | Kamera für die Live-Vorschau der Einstellungen. |
| **Live anzeigen** | Öffnet ein echtes Vorschaufenster dieser Kamera. Es übernimmt Ihre Änderungen (Ecke, Größe, Beschriftung, Stream, Audio …) sofort, noch bevor Sie sie speichern. Es ist angeheftet und zählt nicht zum Fensterlimit. |
| **OK** / **Abbrechen** / **Übernehmen** | Speichern und schließen / verwerfen / speichern und Fenster geöffnet lassen. |
| **Hilfe** | Öffnet dieses Handbuch (auch **F1**). |

## 7. Die Einstellungsdatei

Alle Einstellungen werden in `%APPDATA%\FrigatePiP\config.ini` gespeichert (eine reine Textdatei). Das Einstellungsfenster schreibt sie alle; die Datei benötigen Sie nur für Einstellungen pro Kamera. **Einstellungsdatei (erweitert)…** im Tray-Menü öffnet sie im Editor. Änderungen werden beim Speichern der Datei automatisch übernommen. Kommentare beginnen mit `;`. Ja/Nein-Werte akzeptieren `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Abschnitt | Schlüssel | Standard | Bedeutung |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (Windows-Sprache) oder `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Frigate-Adresse. |
| | `source` | `websocket` | `websocket` oder `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | Vorlage der RTSP-Adresse. |
| `[mqtt]` | `host` | *(leer)* | MQTT-Broker (nur mit `source = mqtt`). |
| | `port` | `1883` | Port des Brokers. |
| | `user`, `password` | *(leer)* | Anmeldedaten. |
| | `prefix` | `frigate` | Topic-Präfix. |
| `[trigger]` | `mode` | `events` | `events` (jedes Objekt), `alerts` (Review-Alarme) oder `reviews` (Alarme und Erkennungen). |
| | `cameras` | *(leer = alle)* | Kommagetrennte Kameranamen, z. B. `front_door, garden`. |
| | `labels` | *(leer = alle)* | Kommagetrennte Objekte, z. B. `person, car`. |
| | `zones` | *(leer = überall)* | Erforderliche Zonen. |
| | `ignore_stationary` | `true` | Unbewegte Objekte ignorieren. |
| | `min_score` | `0` | Mindest-Erkennungssicherheit 0.0–1.0. |
| `[display]` | `monitor` | `primary` | `primary` oder Monitornummer von links (1, 2, 3 …). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Abstand zum Bildschirmrand in Pixeln. |
| | `width` | `480` | Fensterbreite in Pixeln. |
| | `duration` | `15` | Sekunden nach der letzten Bewegung. |
| | `max_duration` | `120` | Maximale Anzeigedauer eines Fensters in Sekunden. |
| | `topmost` | `true` | Immer im Vordergrund. |
| | `sound` | `false` | Benachrichtigungston. |
| | `overlay` | `true` | Beschriftung im Bild. |
| | `max_popups` | `4` | Max. Anzahl Fenster. |
| | `replace` | `true` | `true` = ältestes Fenster ersetzen, `false` = neues Ereignis überspringen. |
| | `skip_fullscreen` | `false` | Bei Vollbildanwendungen nicht anzeigen. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` oder `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` oder `main`. |
| | `auto_main_width` | `800` | Fensterbreite (px), ab der `auto` den Main-Stream verwendet. |
| | `prebuffer` | `false` | Vorpuffer. |
| | `gpu_warm` | `false` | GPU bereithalten. |
| | `camera_audio` | `false` | Kameraton. |

### Einstellungen pro Kamera

Fügen Sie einen Abschnitt `[camera.<name>]` hinzu, wobei `<name>` der Kameraname in Frigate ist:

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
```

## 8. Befehlszeile

| Befehl | Wirkung |
|---|---|
| `frigate-pip.exe` | Startet FrigatePiP (es läuft immer nur eine Instanz). |
| `frigate-pip.exe --settings` | Öffnet das Einstellungsfenster (startet FrigatePiP, falls es nicht läuft). |
| `frigate-pip.exe --test <camera>` | Zeigt sofort ein Vorschaufenster der Kamera, z. B. `--test front_door`. Sie können eine Windows-Verknüpfung mit Tastenkombination für Ihre Lieblingskamera anlegen. |
| `frigate-pip.exe --quit` | Beendet das laufende FrigatePiP. |

Läuft FrigatePiP bereits, wird der Befehl an die laufende Instanz übergeben.

## 9. Sprache

FrigatePiP ist auf Englisch, Polnisch, Deutsch, Spanisch, Portugiesisch und Französisch verfügbar. Mit **Automatisch (Windows)** verwendet es die Windows-Anzeigesprache (*Einstellungen → Zeit und Sprache → Sprache und Region → Windows-Anzeigesprache*) oder Englisch, falls diese Sprache nicht verfügbar ist. Sie können die Sprache unter **Verbindung → Sprache** auch selbst wählen; das Einstellungsfenster wechselt sofort.

## 10. Dateien, Datenschutz und Ressourcen

| Was | Wo |
|---|---|
| Programm | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (Installationsprogramm) |
| Einstellungen | `%APPDATA%\FrigatePiP\config.ini` |
| Protokoll | `%APPDATA%\FrigatePiP\frigate-pip.log` (bis zu 512 KB; das vorherige wird als `frigate-pip.old.log` aufbewahrt) |
| Autostart | Registrierungswert `FrigatePiP` in `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Datenschutz:** FrigatePiP verbindet sich nur mit den Adressen, die Sie eingeben (Frigate, go2rtc und der MQTT-Broker). Es gibt keine Telemetrie, keine Konten und keine Clouddienste. Bilder werden nur angezeigt und nie gespeichert. Die Links unter **Hilfe** öffnen dieses Handbuch auf GitHub in Ihrem Browser.

**Ressourcen:** Während FrigatePiP auf Erkennungen wartet, verbraucht es praktisch keine CPU-Leistung und ca. 5–10 MB Arbeitsspeicher. Video wird nach Möglichkeit von der Grafikkarte decodiert. Ohne nutzbare Grafikkarte (z. B. in manchen virtuellen Maschinen) zeichnet es mit dem Software-Renderer WARP.

## 11. Fehlerbehebung

Beginnen Sie mit **Verbindung testen** in den Einstellungen und mit **Protokoll anzeigen** im Tray-Menü.

**Das Symbol bleibt grau (nicht verbunden)**
- Prüfen Sie die Frigate-Adresse und den Port (5000) in einem Browser auf demselben PC: `http://<frigate>:5000/api/version` sollte die Version anzeigen.
- Unter Docker muss Port 5000 veröffentlicht sein. Port 8971 (mit Anmeldung) wird nicht unterstützt.
- Home-Assistant-Add-on: Aktivieren Sie Port 5000 in den *Netzwerk*-Einstellungen des Add-ons.
- Die Adresse muss mit `http://` beginnen. HTTPS (z. B. über einen Reverse Proxy) wird nicht unterstützt.
- Bei MQTT: Prüfen Sie Broker-Adresse, Anmeldedaten und Topic-Präfix.

**Es erscheinen keine Fenster**
- Ist das Symbol orange? Dann sind die Benachrichtigungen pausiert — wählen Sie **Jetzt fortsetzen**.
- Prüfen Sie die Registerkarte **Erkennung**: Kameras, Objekte und erforderliche Zonen. Sind erforderliche Zonen ausgewählt, müssen Objekte sie betreten.
- Die *Review*-Modi hängen von der Review-Konfiguration in Frigate ab; versuchen Sie **Jedes erkannte Objekt**.
- **Während Spielen oder Präsentationen im Vollbild nicht anzeigen** überspringt Fenster, solange eine Vollbildanwendung im Vordergrund ist.
- **Max. Anzahl Fenster** zusammen mit **Neues Ereignis überspringen** überspringt Erkennungen, wenn alle Fenster geöffnet sind.
- **Min. Erkennungssicherheit** ist möglicherweise zu hoch eingestellt.

**Das Fenster zeigt nur ein Standbild**
- Livevideo benötigt den RTSP-Port 8554 von go2rtc. Prüfen Sie die Zeile **RTSP-Video** von **Verbindung testen**.
- Wenn go2rtc eine Anmeldung erfordert, tragen Sie die Anmeldedaten in die **RTSP-Adresse** ein.
- Installieren Sie für H.265-Kameras die *HEVC-Videoerweiterungen* oder verwenden Sie einen H.264-Sub-Stream.
- Wählen Sie als Notlösung **MJPEG von Frigate**.
- Unter Windows N ohne *Media Feature Pack* ist RTSP-Video nicht verfügbar; FrigatePiP verwendet automatisch MJPEG (im Protokoll: *MJPEG from Frigate instead of RTSP*).

**Das Video startet langsam**
- Aktivieren Sie **Vorpuffer** und **GPU bereithalten**.
- Verkürzen Sie das Schlüsselbildintervall (I-Frame-Intervall / GOP) des Sub-Streams der Kamera auf 1–2 s.

**Kein Kameraton**
- **Kameraton** muss aktiviert und **Livebild** auf **RTSP von go2rtc** eingestellt sein.
- Der go2rtc-Stream muss Audio enthalten — **Verbindung testen** zeigt *Audio … Hz* oder *kein Audio* an.
- Prüfen Sie die Lautstärke von *FrigatePiP* im Lautstärkemixer von Windows.

**Etwas anderes** — Erstellen Sie ein [Issue auf GitHub](https://github.com/maciejgornicz/frigate-pip/issues) und hängen Sie den relevanten Teil des Protokolls an. Entfernen Sie Adressen und Kameranamen, die Sie nicht weitergeben möchten.
