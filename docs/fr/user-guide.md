# FrigatePiP — Guide de l’utilisateur

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP est une petite application Windows de la zone de notification pour [Frigate NVR](https://frigate.video). Lorsque Frigate détecte une personne, une voiture ou tout autre objet, FrigatePiP affiche immédiatement, dans un coin de votre écran, une petite fenêtre en mode image dans l’image avec la vidéo en direct de cette caméra. Quand plus rien ne bouge, la fenêtre se ferme d’elle-même.

Les sections 1 à 11 décrivent l’application Windows. FrigatePiP fonctionne aussi sous Linux — sur le bureau et sur une console sans bureau ; cette version est décrite à part dans la section 12, [Version pour Linux](#12-version-pour-linux).

## Sommaire

1. [Configuration requise](#1-configuration-requise)
2. [Installation](#2-installation)
3. [Premier démarrage](#3-premier-démarrage)
4. [Icône de la zone de notification](#4-icône-de-la-zone-de-notification)
5. [Fenêtres d’aperçu](#5-fenêtres-daperçu)
6. [Fenêtre des paramètres](#6-fenêtre-des-paramètres)
7. [Fichier de paramètres](#7-fichier-de-paramètres)
8. [Ligne de commande](#8-ligne-de-commande)
9. [Langue](#9-langue)
10. [Fichiers, confidentialité et ressources](#10-fichiers-confidentialité-et-ressources)
11. [Dépannage](#11-dépannage)
12. [Version pour Linux](#12-version-pour-linux)

---

## 1. Configuration requise

- **Windows 10 (version 1809 ou ultérieure) ou Windows 11**, 64 bits.
- **Frigate** accessible depuis votre PC par le réseau. Frigate 0.14 ou ultérieur est recommandé ; les modes *Review* nécessitent la version 0.14 ou ultérieure.
- L’**API** de Frigate doit être accessible depuis le PC : le **port 5000** non authentifié (`http://`) ou le **port 8971** authentifié (`https://`, avec un utilisateur Frigate — le rôle *viewer* suffit ; créez-le dans *Paramètres → Utilisateurs* de Frigate). Sous Docker, publiez le port, p. ex. `- "5000:5000"` ou `- "8971:8971"`. Un proxy inverse devant Frigate fonctionne aussi.
- **Module complémentaire Frigate pour Home Assistant :** le port 5000 est désactivé par défaut — activez-le dans les paramètres *Réseau* du module. Vous pouvez aussi utiliser le port 8971 avec un utilisateur Frigate.
- Pour une vidéo en direct fluide : le **serveur RTSP go2rtc intégré à Frigate, port 8554** (`- "8554:8554"`). Sans lui, vous pouvez toujours utiliser MJPEG ou des instantanés (voir [Image en direct](#image-en-direct)).
- Facultatif : un **broker MQTT** utilisé par Frigate, si vous préférez MQTT au WebSocket Frigate.
- Pour les caméras H.265 (HEVC), Windows a besoin d’un décodeur HEVC. La plupart des PC dotés d’un pilote graphique récent en possèdent un ; sinon, installez *Extensions vidéo HEVC* depuis le Microsoft Store, ou utilisez un flux sub en H.264. Si aucun décodeur n’est disponible, FrigatePiP affiche automatiquement le MJPEG de Frigate à la place.
- **Les éditions N de Windows** (par exemple Windows 11 Pro N) ne comportent pas Media Foundation tant que vous n’avez pas ajouté la fonctionnalité facultative de Windows *Media Feature Pack*. FrigatePiP fonctionne quand même sans elle, mais l’image en direct arrive alors en MJPEG depuis Frigate (environ 5 images par seconde) et le son AAC de la caméra n’est pas disponible.

## 2. Installation

### Avec le programme d’installation (recommandé)

1. Téléchargez `FrigatePiP-<version>-installer.exe` depuis la page [Releases](https://github.com/maciejgornicz/frigate-pip/releases).
2. Exécutez-le. Le programme d’installation ne nécessite pas de droits d’administrateur ; il installe FrigatePiP pour l’utilisateur actuel uniquement, dans `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Indiquez si vous voulez **Lancer FrigatePiP au démarrage de Windows** (option activée par défaut) et créer une icône sur le Bureau.
4. À la fin, laissez **Exécuter FrigatePiP** coché. L’icône apparaît dans la zone de notification et la fenêtre des paramètres s’ouvre au premier démarrage.

Le programme d’installation et l’application ne sont pas signés avec un certificat de signature de code. Windows SmartScreen peut afficher *« Windows a protégé votre ordinateur »* ; cliquez sur **Informations complémentaires → Exécuter quand même**. Vous pouvez vérifier la somme de contrôle SHA-256 publiée avec chaque version.

### Version portable (sans installation)

Téléchargez `FrigatePiP-<version>-portable.exe` depuis la page Releases et exécutez-le depuis n’importe quel dossier. Les paramètres sont stockés au même endroit qu’avec le programme d’installation (voir [Fichiers](#10-fichiers-confidentialité-et-ressources)). Utilisez **Lancer FrigatePiP au démarrage de Windows** dans les paramètres pour le démarrer automatiquement.

### Mise à jour

FrigatePiP recherche une nouvelle version une fois par jour (**Rechercher les mises à jour** dans les paramètres). Lorsqu’une version est disponible, une notification apparaît, et le menu de la zone de notification comme la fenêtre des paramètres proposent **Mettre à jour vers …** : FrigatePiP télécharge le programme d’installation depuis GitHub, vérifie sa somme de contrôle SHA-256 et l’exécute — la mise à jour prend quelques secondes et conserve vos paramètres. **Vérifier maintenant** dans les paramètres recherche immédiatement une nouvelle version. Vous pouvez aussi exécuter vous-même le programme d’installation d’une nouvelle version. La version portable ouvre seulement la page de la version : téléchargez le nouveau fichier exe et remplacez l’ancien.

### Désinstallation

**Paramètres → Applications → Applications installées → FrigatePiP → Désinstaller** (ou *Ajouter ou supprimer des programmes*). Le programme de désinstallation ferme FrigatePiP et supprime le programme, les raccourcis et l’entrée de démarrage automatique. Vos paramètres et le journal situés dans `%APPDATA%\FrigatePiP` sont conservés ; supprimez ce dossier pour les effacer également.

## 3. Premier démarrage

1. Au premier démarrage, la fenêtre des paramètres s’ouvre sur l’onglet **Connexion**.
2. Saisissez l’**Adresse de Frigate**, p. ex. `http://192.168.1.10:5000`.
3. Cliquez sur **Tester la connexion**. FrigatePiP vérifie, sans rien enregistrer :
   - l’API Frigate et sa version,
   - la liste des caméras,
   - la source des événements (WebSocket ou MQTT),
   - la vidéo RTSP de go2rtc, y compris le codec et la présence d’audio sur la caméra.

   Chaque ligne commence par ✔ (fonctionne) ou ✖ (problème, avec une indication).
4. Cliquez sur **OK**. Dès lors, FrigatePiP écoute les détections en arrière-plan.
5. Pour voir à quoi ressemble une fenêtre, cliquez avec le bouton droit sur l’icône de la zone de notification et choisissez **Afficher la caméra → (une caméra)**, ou cochez **Afficher en direct** en bas de la fenêtre des paramètres.

## 4. Icône de la zone de notification

L’icône dans la zone de notification indique l’état :

| Couleur | Signification |
|---|---|
| Bleu | Connecté à Frigate, en attente de détections |
| Gris | Non connecté (FrigatePiP réessaie automatiquement) |
| Orange | Les notifications sont suspendues |

Survolez l’icône pour voir l’état (par exemple *Connecté : http://192.168.1.10:5000*).

- Un **clic gauche** ouvre la fenêtre des paramètres.
- Un **clic droit** ouvre le menu :

| Élément de menu | Action |
|---|---|
| *(ligne d’état)* | État de la connexion ou temps de pause restant (à titre informatif). |
| **Mettre à jour vers …** | Apparaît lorsqu’une nouvelle version est disponible : la télécharge et l’installe (voir [Mise à jour](#mise-à-jour)). |
| **Paramètres…** | Ouvre la fenêtre des paramètres. |
| **Afficher la caméra** | Ouvre immédiatement une fenêtre d’aperçu de la caméra choisie, même lorsque les notifications sont suspendues. Pratique pour vérifier l’image ou simplement pour regarder. |
| **Suspendre les notifications** | **Pendant 15 minutes**, **Pendant 1 heure** ou **Jusqu’à la reprise**. Pendant la pause, les détections n’ouvrent pas de fenêtres. L’élément affiche **En pause** tant qu’une pause est active, et **Reprendre maintenant** y met fin. |
| **Fermer les fenêtres d’aperçu** | Ferme toutes les fenêtres d’aperçu ouvertes, y compris celles qui sont épinglées. |
| **Ouvrir Frigate** | Ouvre l’interface web de Frigate dans votre navigateur. |
| **Aide** | Ouvre ce guide. |
| **Afficher le journal** | Ouvre le fichier journal dans le Bloc-notes (utile lorsque quelque chose ne fonctionne pas). |
| **Fichier de paramètres (avancé)…** | Ouvre `config.ini` dans le Bloc-notes. Les modifications enregistrées s’appliquent immédiatement. |
| **Quitter** | Ferme FrigatePiP. |

## 5. Fenêtres d’aperçu

Lorsqu’une détection correspond à vos paramètres, une fenêtre d’aperçu apparaît dans le coin de l’écran choisi :

1. La fenêtre apparaît instantanément, quelques millisecondes après le message de Frigate.
2. Elle affiche d’abord le dernier instantané de la caméra pris par Frigate, puis passe à la vidéo en direct dès que le flux fournit une image (généralement en moins d’une seconde, presque instantanément avec le [préchargement](#préchargement)).
3. La légende (si elle est activée) affiche le nom de la caméra, l’objet (par exemple *personne*), la sous-étiquette (par exemple un visage ou une plaque reconnus), la confiance et l’heure. Un **point rouge** signifie que l’image est en direct.

Les fenêtres d’aperçu ne prennent jamais le focus du clavier : elles n’interrompent donc ni la saisie ni les jeux.

### Actions de la souris

| Action | Effet |
|---|---|
| **Glisser** | Déplace la fenêtre. Elle reste là où vous l’avez placée jusqu’à sa fermeture. |
| **Clic** | **Épingle** la fenêtre : elle reste ouverte jusqu’à ce que vous la fermiez (la légende affiche *épinglé*). Cliquez à nouveau pour la désépingler. |
| **Molette de la souris** | Agrandit ou réduit la fenêtre. |
| **Ctrl + molette de la souris** | Zoome sur l’image (jusqu’à 8×) ou dézoome autour du pointeur ; la légende affiche le zoom, par ex. *2.0×*. Le détail dépend du flux : le flux main (`stream = main`) en a le plus. |
| **Ctrl + glisser** | Sur une image zoomée : déplace l’image pour en voir une autre partie. Sans zoom, c’est la fenêtre qui se déplace, comme d’habitude. |
| **×** (coin supérieur droit) | Ferme immédiatement la fenêtre. Le bouton apparaît lorsque vous pointez la souris sur la fenêtre. |
| **Haut-parleur** (à gauche de ×) | Coupe le son de la fenêtre ; un nouveau clic le rétablit. Le bouton apparaît lorsque vous pointez une fenêtre qui diffuse le son de la caméra ; tant que le son est coupé, le haut-parleur barré reste visible. |
| **Curseur de volume** (sous le haut-parleur) | Règle le volume de cette fenêtre : faites-le glisser ou cliquez dessus. Il apparaît avec le haut-parleur ; monter le volume réactive aussi le son coupé. Sous Linux, le nouveau volume s’entend juste après avoir relâché. |
| **Relecture** ⟲ (à gauche des autres boutons) | Rejoue le début de l’événement à vitesse normale, sans son ; la légende affiche *Relecture −12 s*, c’est-à-dire le retard de l’image sur le direct. Avec le [préchargement](#préchargement), la relecture commence 5 s avant l’ouverture de la fenêtre ; sans lui, à l’ouverture de la fenêtre. Cliquez à nouveau pour revenir aussitôt à la vidéo en direct ; après la relecture, elle revient d’elle-même. Le bouton apparaît quand vous pointez une fenêtre avec vidéo en direct ; la fenêtre reste ouverte pendant la relecture. |
| **Clic droit** | Ferme la fenêtre. |
| **Double-clic** | Ouvre cette caméra dans l’interface web de Frigate. |
| **Survol** | Tant que le pointeur de la souris se trouve sur la fenêtre, celle-ci ne se ferme pas. Une fois le pointeur éloigné, elle reste encore au moins 3 secondes. |

### Quand une fenêtre se ferme-t-elle ?

- Après le nombre de secondes défini dans **Durée après dernier mouvement**, compté depuis le dernier mouvement de l’objet (ou de tout autre objet correspondant sur cette caméra).
- Au plus tard après le nombre de secondes défini dans **au plus**, même si l’objet continue de bouger.
- Jamais tant qu’elle est épinglée ou que le pointeur de la souris se trouve dessus.
- Jamais pendant une relecture ; ensuite, elle reste encore ouverte au moins 5 secondes.

### Plusieurs caméras

Chaque caméra a au plus une fenêtre. Les nouvelles détections sur une caméra qui a déjà une fenêtre ne font que la prolonger et mettre à jour la légende. Les fenêtres des différentes caméras s’empilent à partir du coin choisi. **Nombre max. de fenêtres** limite le nombre de fenêtres ouvertes simultanément ; **Si toutes sont occupées** détermine si une nouvelle caméra remplace la fenêtre la plus ancienne (au même endroit de l’écran ; les fenêtres épinglées ne sont jamais remplacées) ou est ignorée.

## 6. Fenêtre des paramètres

Ouvrez-la d’un clic gauche sur l’icône de la zone de notification ou avec **Paramètres…** dans le menu. **Chaque option possède une description : survolez-la avec la souris.** Appuyez sur **F1** ou cliquez sur **Aide** pour ouvrir ce guide.

La plupart des modifications s’appliquent **immédiatement** aux fenêtres d’aperçu ouvertes, ce qui vous permet de voir le résultat avant d’enregistrer. **OK** enregistre et ferme, **Appliquer** enregistre et garde la fenêtre ouverte, **Annuler** (ou la fermeture de la fenêtre) abandonne les modifications non enregistrées.

### 6.1 Connexion

| Option | Description |
|---|---|
| **Adresse de Frigate** | Adresse du serveur Frigate avec le port : `http://192.168.1.10:5000` (port 5000, sans authentification) ou `https://192.168.1.10:8971` (port 8971, avec l’utilisateur Frigate ci-dessous). L’adresse d’un proxy inverse fonctionne aussi. |
| **Utilisateur / mot de passe Frigate** | Uniquement pour le port 8971 ou un proxy inverse qui demande une connexion à Frigate : un utilisateur Frigate (*Paramètres → Utilisateurs* dans Frigate) ; le rôle *viewer* suffit. Laissez vide pour le port 5000. FrigatePiP se connecte, garde le jeton donné par Frigate et se reconnecte quand il expire. Le mot de passe est enregistré dans `config.ini`, comme celui de MQTT. |
| **Source des événements** | **WebSocket Frigate (recommandé)** — FrigatePiP se connecte directement à Frigate ; rien d’autre n’est nécessaire. **MQTT** — les détections sont lues depuis votre broker MQTT (celui configuré dans Frigate). Les deux donnent le même résultat ; le WebSocket est généralement un peu plus rapide. |
| **Serveur MQTT**, **port** | Nom d’hôte ou adresse IP et port (généralement 1883) du broker MQTT. Utilisé uniquement avec la source MQTT. |
| **Utilisateur / mot de passe** | Identifiants MQTT. Laissez vide si le broker autorise l’accès anonyme. |
| **Préfixe des sujets MQTT** | `mqtt.topic_prefix` de la configuration de Frigate, généralement `frigate`. |
| **Adresse RTSP (go2rtc)** | Modèle de l’adresse de la vidéo en direct. `{host}` est remplacé par l’hôte de l’adresse Frigate et `{stream}` par le nom du flux go2rtc. La valeur par défaut `rtsp://{host}:8554/{stream}` convient à une installation standard de Frigate. Si go2rtc exige une authentification, utilisez `rtsp://user:password@{host}:8554/{stream}`. |
| **Tester la connexion** | Vérifie les valeurs saisies ci-dessus (Frigate, caméras, événements, vidéo et audio RTSP) sans les enregistrer. |
| **Langue** | Langue de FrigatePiP. **Automatique (Windows)** suit la langue d’affichage de Windows ; l’anglais est utilisé lorsque cette langue n’est pas disponible. Voir [Langue](#9-langue). |
| **Lancer FrigatePiP au démarrage de Windows** | Lance FrigatePiP automatiquement à l’ouverture de votre session Windows. |
| **Rechercher les mises à jour** | Une fois par jour, FrigatePiP demande à GitHub si une nouvelle version existe et, le cas échéant, affiche une notification et propose la mise à jour. Aucune donnée sur vous ou vos caméras n’est envoyée. |
| **Vérifier maintenant** / **Mettre à jour vers …** | Recherche immédiatement une nouvelle version ; si elle en trouve une, le bouton la télécharge et l’installe (vos paramètres sont conservés). Le texte à côté affiche le résultat. |

### 6.2 Détection

| Option | Description |
|---|---|
| **Afficher une fenêtre pour** | Messages de Frigate qui ouvrent une fenêtre : **Chaque objet détecté (le plus rapide)** — chaque objet suivi par Frigate ; **Alertes Review uniquement** — uniquement les éléments Review de gravité *alert* ; **Alertes et détections Review** — tous les éléments Review. Les éléments Review suivent les règles d’alerte et de détection (étiquettes et zones requises) définies dans la configuration `review` de Frigate et arrivent un peu plus tard que les événements d’objets. |
| **Caméras** | Caméras pouvant ouvrir une fenêtre. Les caméras non cochées sont ignorées. Lorsque toutes les caméras sont cochées, les caméras ajoutées ultérieurement à Frigate sont également incluses. |
| **Objets** | Types d’objets (étiquettes) pouvant ouvrir une fenêtre, p. ex. personne, voiture, chien. La liste affiche les objets suivis par vos caméras. Lorsque tout est coché, tous les types d’objets sont pris en compte. |
| **Zones requises** | Si vous cochez des zones, une fenêtre ne s’ouvre que lorsque l’objet entre dans l’une d’elles. Si aucune n’est cochée, l’objet peut se trouver n’importe où dans l’image. |
| **Ignorer les objets immobiles** | Les objets que Frigate considère comme immobiles (par exemple une voiture garée) n’ouvrent pas de fenêtres et ne les maintiennent pas ouvertes. S’applique à *Chaque objet détecté*. |
| **Confiance minimale** | Les détections dont la confiance est inférieure sont ignorées. 0 % signifie que ce sont les seuils propres à Frigate qui décident. S’applique à *Chaque objet détecté*. |

Les faux positifs de Frigate sont toujours ignorés. Chaque objet n’ouvre une fenêtre qu’une seule fois ; lorsqu’il bouge à nouveau, il ne fait que prolonger la fenêtre ouverte.

Les **événements manuels** ouvrent aussi une fenêtre : les événements créés par l’API de Frigate (`POST /api/events/<caméra>/<label>/create`, par exemple par une automatisation Home Assistant quand on sonne à la porte) et les éléments d’une caméra LPR dédiée. La légende affiche le label de l’événement. Cela fonctionne avec **Chaque objet détecté (le plus rapide)** et **Alertes et détections Review**, et avec **Alertes Review uniquement** quand Frigate fait de l’événement une alerte (son label fait partie des labels d’alerte de Review, par défaut `person` et `car`). Pour tester dans PowerShell : `Invoke-RestMethod -Method Post -ContentType application/json -Body '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create` ; sous Linux : `curl -X POST -H "Content-Type: application/json" -d '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`. Les filtres de caméras, d’objets et de zones s’appliquent comme aux autres détections (un filtre d’objets `person, car` masque un événement `doorbell`) ; la confiance minimale non. Frigate ne crée l’élément de Review auquel FrigatePiP réagit que si la caméra enregistre (`record` activé). Un événement manuel créé alors que l’élément de Review de la caméra est encore ouvert (jusqu’au délai de coupure de l’alerte, 40 s par défaut après l’événement précédent) est ajouté à cet élément, et Frigate ne l’annonce que lorsqu’un objet bouge sur la caméra — pour tester, attendez donc environ une minute entre deux événements.

### 6.3 Fenêtre d’aperçu

| Option | Description |
|---|---|
| **Écran** | Écran sur lequel les fenêtres apparaissent : **Principal** ou un écran précis (numérotés à partir de la gauche, avec leur résolution). |
| **Coin de l’écran** | Coin dans lequel les fenêtres apparaissent. Les fenêtres suivantes s’empilent à partir de ce coin. |
| **Largeur de la fenêtre** | Largeur en pixels (pour une mise à l’échelle de 100 % ; FrigatePiP l’adapte à la mise à l’échelle de l’écran). La hauteur suit les proportions de l’image de la caméra. Vous pouvez aussi modifier la taille d’une seule fenêtre avec la molette de la souris. |
| **Distance au bord** | Espace entre les fenêtres et les bords de l’écran (et entre les fenêtres). |
| **Durée après dernier mouvement** | Nombre de secondes pendant lesquelles une fenêtre reste ouverte après le dernier mouvement. |
| **au plus** | Durée maximale d’affichage d’une fenêtre, en secondes, même si l’objet continue de bouger (sauf si elle est épinglée). |
| **Nombre max. de fenêtres** | Nombre de fenêtres (caméras) pouvant être ouvertes en même temps (1–8). |
| **Si toutes sont occupées** | **Remplacer la fenêtre la plus ancienne (même emplacement)** — la nouvelle caméra prend la place de la plus ancienne fenêtre non épinglée ; **Ignorer le nouvel événement** — la nouvelle détection est ignorée. |
| **Toujours au premier plan** | Maintient les fenêtres au-dessus de toutes les autres fenêtres. |
| **Légende sur l’image** | Affiche la caméra, l’objet, la confiance et l’heure sur l’image. |
| **Son de notification** | Joue le son Windows *Astérisque* à l’apparition d’une nouvelle fenêtre. Il ne s’agit pas du son de la caméra (voir [Son de la caméra](#son-de-la-caméra)). |
| **Ne pas afficher pendant les jeux ou présentations en plein écran** | Aucune fenêtre n’apparaît pendant l’exécution d’un jeu, d’une vidéo ou d’une présentation en plein écran (les détections sont ignorées, et non différées). |

### 6.4 Vidéo et audio

#### Image en direct

| Choix | Description |
|---|---|
| **RTSP depuis go2rtc** (par défaut) | Vidéo fluide à la fréquence d’images de la caméra avec décodage matériel (H.264 et H.265), faible latence. Nécessite le port 8554. |
| **MJPEG depuis Frigate** | Environ 5 images par seconde à la résolution de détection, via l’API de Frigate. À utiliser lorsque RTSP n’est pas disponible. |
| **Instantanés actualisés uniquement** | Uniquement les instantanés de Frigate, actualisés régulièrement. L’option la plus légère. |

#### Flux

| Choix | Description |
|---|---|
| **Automatique (selon la taille de la fenêtre)** | Le flux **sub** léger pour les fenêtres normales, le flux **main** en pleine résolution lorsque la fenêtre est plus large que la valeur **Main dès une largeur de** (en pixels physiques). |
| **Sub (léger)** | Toujours le flux sub. |
| **Main (pleine résolution)** | Toujours le flux main. |

FrigatePiP trouve les flux go2rtc de chaque caméra dans la configuration de Frigate (`live.streams`, `live.stream_name`, ou les flux go2rtc nommés `<camera>` et `<camera>_sub`). Vous pouvez les définir vous-même dans le [fichier de paramètres](#paramètres-par-caméra).

#### Son de la caméra

L’option **Son de la caméra** joue le son de la caméra (AAC ou G.711 issu du flux RTSP) dans chaque fenêtre ouverte. Il est traité séparément et ne retarde donc jamais la fenêtre ni la vidéo. Le volume se règle dans le Mélangeur de volume de Windows (*FrigatePiP*). Cette option nécessite **RTSP depuis go2rtc**. Pour couper le son d’une seule fenêtre, cliquez sur le bouton haut-parleur dans son coin supérieur droit (voir [Actions de la souris](#actions-de-la-souris)). **Volume** définit le niveau de départ d’une nouvelle fenêtre (0–100 %, `camera_volume`), par exemple pour atténuer le bruit du vent ; chaque fenêtre a aussi son propre curseur.

#### Préchargement

Le **Préchargement** maintient une connexion au flux sub de chaque caméra, de sorte que la vidéo en direct démarre immédiatement (environ 0,15 s) au lieu d’attendre la prochaine image clé (jusqu’à environ 1 s, selon l’intervalle entre images clés de la caméra). Coût : environ 0,1–0,5 Mbit/s de trafic sur le réseau local par caméra, en permanence. Il garde aussi en mémoire les 30 dernières secondes de chaque caméra (autant que 30 s de son flux sub : environ 0,2 à 2 Mo par caméra), pour que la [relecture](#actions-de-la-souris) puisse commencer 5 s avant l’événement.

#### GPU prêt

L’option **GPU prêt** maintient le périphérique graphique initialisé, de sorte que la vidéo démarre environ 130 ms plus tôt. Coût : environ 1 % d’un cœur CPU au repos (thread du pilote graphique).

### 6.5 Bas de la fenêtre

| Élément | Description |
|---|---|
| **Aperçu :** caméra | Caméra utilisée pour l’aperçu en direct des paramètres. |
| **Afficher en direct** | Ouvre une vraie fenêtre d’aperçu de cette caméra. Elle reflète immédiatement vos modifications (coin, taille, légende, flux, audio…), avant même leur enregistrement. Elle est épinglée et n’est pas comptée dans la limite de fenêtres. |
| **OK** / **Annuler** / **Appliquer** | Enregistrer et fermer / abandonner / enregistrer et garder la fenêtre ouverte. |
| **Aide** | Ouvre ce guide (également **F1**). |

## 7. Fichier de paramètres

Tous les paramètres sont stockés dans `%APPDATA%\FrigatePiP\config.ini` (un fichier texte brut). La fenêtre des paramètres les écrit tous ; vous n’avez besoin du fichier que pour les paramètres par caméra. **Fichier de paramètres (avancé)…** dans le menu de la zone de notification l’ouvre dans le Bloc-notes. Les modifications sont appliquées automatiquement à l’enregistrement du fichier. Les commentaires commencent par `;`. Les valeurs oui/non acceptent `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Section | Clé | Par défaut | Signification |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (langue de Windows) ou `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Rechercher une nouvelle version une fois par jour. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Adresse de Frigate. |
| | `user`, `password` | *(vide)* | Connexion à Frigate pour le port 8971 (vide pour le port 5000). |
| | `source` | `websocket` | `websocket` ou `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | Modèle d’adresse RTSP. |
| `[mqtt]` | `host` | *(vide)* | Broker MQTT (uniquement avec `source = mqtt`). |
| | `port` | `1883` | Port du broker. |
| | `user`, `password` | *(vide)* | Identifiants. |
| | `prefix` | `frigate` | Préfixe des sujets. |
| `[trigger]` | `mode` | `events` | `events` (chaque objet), `alerts` (alertes Review) ou `reviews` (alertes et détections). |
| | `cameras` | *(vide = toutes)* | Noms de caméras séparés par des virgules, p. ex. `front_door, garden`. |
| | `labels` | *(vide = tous)* | Objets séparés par des virgules, p. ex. `person, car`. |
| | `zones` | *(vide = n’importe où)* | Zones requises. |
| | `ignore_stationary` | `true` | Ignorer les objets immobiles. |
| | `min_score` | `0` | Confiance minimale de 0.0 à 1.0. |
| `[display]` | `monitor` | `primary` | `primary` ou numéro de l’écran à partir de la gauche (1, 2, 3…). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Distance au bord de l’écran en pixels. |
| | `width` | `480` | Largeur de la fenêtre en pixels. |
| | `duration` | `15` | Secondes après le dernier mouvement. |
| | `max_duration` | `120` | Durée maximale d’affichage d’une fenêtre, en secondes. |
| | `topmost` | `true` | Toujours au premier plan. |
| | `sound` | `false` | Son de notification. |
| | `overlay` | `true` | Légende sur l’image. |
| | `max_popups` | `4` | Nombre max. de fenêtres. |
| | `replace` | `true` | `true` = remplacer la fenêtre la plus ancienne, `false` = ignorer le nouvel événement. |
| | `skip_fullscreen` | `false` | Ne pas afficher pendant les applications en plein écran. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` ou `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` ou `main`. |
| | `auto_main_width` | `800` | Largeur de fenêtre (px) à partir de laquelle `auto` utilise le flux main. |
| | `prebuffer` | `false` | Préchargement. |
| | `gpu_warm` | `false` | GPU prêt. |
| | `camera_audio` | `false` | Son de la caméra. |
| | `camera_volume` | `100` | Volume d’une nouvelle fenêtre, 0–100 %. |

### Paramètres par caméra

Ajoutez une section `[camera.<name>]`, où `<name>` est le nom de la caméra dans Frigate :

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
```

## 8. Ligne de commande

| Commande | Effet |
|---|---|
| `frigate-pip.exe` | Démarre FrigatePiP (une seule instance s’exécute à la fois). |
| `frigate-pip.exe --settings` | Ouvre la fenêtre des paramètres (démarre FrigatePiP s’il n’est pas en cours d’exécution). |
| `frigate-pip.exe --test <camera>` | Affiche immédiatement une fenêtre d’aperçu de la caméra, p. ex. `--test front_door`. Vous pouvez créer un raccourci Windows doté d’une touche de raccourci pour votre caméra préférée. |
| `frigate-pip.exe --quit` | Ferme l’instance de FrigatePiP en cours d’exécution. |

Si FrigatePiP est déjà en cours d’exécution, la commande lui est transmise.

## 9. Langue

FrigatePiP est disponible en anglais, polonais, allemand, espagnol, portugais et français. Avec **Automatique (Windows)**, il utilise la langue d’affichage de Windows (*Paramètres → Heure et langue → Langue et région → Langue d’affichage de Windows*), ou l’anglais si cette langue n’est pas disponible. Vous pouvez choisir vous-même une langue dans **Connexion → Langue** ; la fenêtre des paramètres change de langue immédiatement.

## 10. Fichiers, confidentialité et ressources

| Élément | Emplacement |
|---|---|
| Programme | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (programme d’installation) |
| Paramètres | `%APPDATA%\FrigatePiP\config.ini` |
| Journal | `%APPDATA%\FrigatePiP\frigate-pip.log` (jusqu’à 512 Ko ; le précédent est conservé sous le nom `frigate-pip.old.log`) |
| Démarrage automatique | Valeur de Registre `FrigatePiP` dans `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Confidentialité :** FrigatePiP se connecte uniquement aux adresses que vous saisissez (Frigate, go2rtc et le broker MQTT) et, sauf si vous désactivez **Rechercher les mises à jour**, une fois par jour à GitHub pour rechercher une nouvelle version. Il n’a ni télémétrie, ni comptes, ni services cloud. Les images sont uniquement affichées et jamais enregistrées. Les liens **Aide** ouvrent ce guide sur GitHub dans votre navigateur.

**Ressources :** en attente de détections, FrigatePiP n’utilise pratiquement pas de CPU et environ 5–10 Mo de mémoire. La vidéo est décodée par la carte graphique lorsque c’est possible. Sans carte graphique utilisable (par exemple dans certaines machines virtuelles), il dessine avec le moteur de rendu logiciel WARP.

## 11. Dépannage

Commencez par **Tester la connexion** dans les paramètres et par **Afficher le journal** dans le menu de la zone de notification.

**L’icône reste grise (non connecté)**
- Vérifiez l’adresse de Frigate et le port dans un navigateur sur le même PC : `http://<frigate>:5000/api/version` (ou `https://<frigate>:8971/api/version` après connexion) doit afficher la version.
- Sous Docker, le port (5000 ou 8971) doit être publié.
- Module complémentaire Home Assistant : activez le port 5000 dans les paramètres *Réseau* du module ou utilisez le port 8971 avec un utilisateur Frigate.
- Port 8971 ou proxy inverse : saisissez l’utilisateur et le mot de passe Frigate ; **Tester la connexion** indique si la connexion fonctionne. Un certificat auto-signé (celui de Frigate sur le port 8971) est approuvé la première fois et mémorisé dans `certificates.txt` (à côté de `config.ini`) ; si le serveur en présente un autre plus tard, FrigatePiP refuse la connexion — si vous avez remplacé le certificat vous-même, supprimez sa ligne dans ce fichier.
- Avec MQTT : vérifiez l’adresse du broker, les identifiants et le préfixe des sujets.

**Aucune fenêtre n’apparaît**
- L’icône est-elle orange ? Les notifications sont suspendues — choisissez **Reprendre maintenant**.
- Vérifiez l’onglet **Détection** : caméras, objets et zones requises. Si des zones requises sont cochées, les objets doivent y entrer.
- Les modes *Review* dépendent de la configuration review de Frigate ; essayez **Chaque objet détecté**.
- **Ne pas afficher pendant les jeux ou présentations en plein écran** ignore les fenêtres lorsqu’une application en plein écran est au premier plan.
- **Nombre max. de fenêtres** combiné à **Ignorer le nouvel événement** ignore les détections lorsque toutes les fenêtres sont ouvertes.
- La **Confiance minimale** est peut-être trop élevée.

**La fenêtre n’affiche qu’une image fixe**
- La vidéo en direct nécessite le port RTSP 8554 de go2rtc. Vérifiez la ligne **Vidéo RTSP** de **Tester la connexion**.
- Si go2rtc exige une authentification, indiquez les identifiants dans l’**Adresse RTSP**.
- Pour les caméras H.265, installez *Extensions vidéo HEVC*, ou utilisez un flux sub en H.264.
- En guise de solution de contournement, sélectionnez **MJPEG depuis Frigate**.
- Sous Windows N sans *Media Feature Pack*, la vidéo RTSP n’est pas disponible ; FrigatePiP utilise automatiquement le MJPEG (le journal indique *MJPEG from Frigate instead of RTSP*).

**La vidéo démarre lentement**
- Activez **Préchargement** et **GPU prêt**.
- Réduisez l’intervalle entre images clés (intervalle I-frame / GOP) du flux sub de la caméra à 1–2 s.

**L’image d’une caméra est en retard sur celle d’une autre**
- Les caméras ne partagent pas d’horloge : FrigatePiP ne peut donc pas aligner leurs images.
- Chaque fenêtre laisse une ligne dans le journal à sa fermeture : *timing - 73 frames, picture in 3 ms (max 96), late up to 177 ms, longest gap 238 ms*. *picture in* est le temps passé dans FrigatePiP, *late up to* le retard maximal des images sur le rythme de leur flux et *longest gap* la plus longue pause entre deux images.
- Des valeurs faibles avec une image toujours en retard signifient que le retard apparaît avant que la vidéo n’atteigne FrigatePiP : vérifiez les réglages du flux de la caméra. Des valeurs élevées désignent le réseau ou, pour *picture in*, le PC.

**Pas de son de la caméra**
- **Son de la caméra** doit être coché et **Image en direct** réglé sur **RTSP depuis go2rtc**.
- Le flux go2rtc doit contenir de l’audio — **Tester la connexion** affiche *audio … Hz* ou *sans audio*.
- Vérifiez le volume de *FrigatePiP* dans le Mélangeur de volume de Windows.

**FrigatePiP se ferme tout seul**
- Chaque ligne du journal commence par la date et l’heure. Un plantage de FrigatePiP laisse une dernière ligne commençant par *Crash in thread* : joignez-la à un ticket (voir ci-dessous).
- Si le journal s’arrête simplement, sans *Shutting down* ni *Crash*, autre chose a fermé le programme : l’arrêt ou une mise à jour de Windows, ou la fermeture du programme qui l’a lancé (par exemple une fenêtre de terminal).

**Autre problème** — ouvrez un [ticket sur GitHub](https://github.com/maciejgornicz/frigate-pip/issues) et joignez la partie pertinente du journal. Supprimez les adresses et les noms de caméras que vous ne souhaitez pas partager.

---

## 12. Version pour Linux

FrigatePiP fonctionne aussi sous Linux — sur un **bureau** (GNOME, KDE, Xfce…) comme sous Windows, et **sans bureau**, par exemple sur un Raspberry Pi relié à un écran mural, ou sur un serveur auquel vous accédez par SSH. Il affiche les mêmes fenêtres d’aperçu, dans l’un des quatre modes suivants :

| Mode | Où apparaissent les fenêtres | Fonctionne en arrière-plan ? | Utilisation typique | Lancement |
|---|---|---|---|---|
| **Fenêtres sur le bureau** | Sur le bureau (X11, ou Wayland avec XWayland) : fenêtres sans bordure au-dessus de toutes les autres, dans le coin choisi, comme sous Windows | **Oui** — sans terminal ; depuis le menu des applications ou avec la session du bureau | Un PC ou un portable sous Linux | Menu → **FrigatePiP**, ou `frigate-pip` sur le bureau — voir [Fenêtres sur le bureau](#fenêtres-sur-le-bureau) |
| **Framebuffer** | Sur l’écran d’une console Linux sans X ni Wayland, dans le coin choisi, par-dessus le texte de la console | **Oui** — en tant que service systemd | Un Raspberry Pi ou un autre ordinateur doté d’un écran mais sans bureau (affichage mural) | `frigate-pip --output framebuffer`, en tant que service : voir [Démarrage automatique](#démarrage-automatique-systemd) |
| **Terminal** | Dans la fenêtre de terminal où s’exécute FrigatePiP : une ligne de légende, l’image et une ligne d’état | **Non** — le terminal affiche FrigatePiP tant qu’il s’exécute (comme `top`) | Une fenêtre de terminal ou un volet tmux laissé ouvert comme moniteur de caméras, y compris via SSH | `frigate-pip` |
| **Popups tmux** | Dans une popup tmux, par-dessus ce que vous faites dans tmux ; elle se ferme d’elle-même | **Oui** — dans une fenêtre tmux masquée | Vous travaillez dans tmux (en local ou via SSH) et voulez que les caméras s’affichent d’elles-mêmes | `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`, puis travaillez dans tmux — voir [Popups tmux](#popups-tmux) |

**Sur un PC Windows — y compris lorsque vous travaillez dans WSL — utilisez l’application Windows :** elle s’exécute dans la zone de notification et ses fenêtres apparaissent par-dessus tout le reste, terminaux compris. La version Linux dans WSL ne sert qu’à faire des essais.

Lorsqu’une fenêtre se ferme sur le framebuffer, ce qui se trouvait dessous réapparaît. Dans un terminal et dans les popups tmux, l’image est dessinée avec le **protocole graphique Kitty** (Kitty, WezTerm, Ghostty), avec **Sixel** (foot, Konsole, xterm, Windows Terminal 1.22+) ou, dans tout terminal prenant en charge les couleurs 24 bits, avec des demi-blocs colorés (basse résolution ; les popups tmux utilisent toujours les demi-blocs).

La vidéo est décodée par le programme **ffmpeg** ; le client RTSP intégré de FrigatePiP l’alimente, si bien qu’une image apparaît quelques millisecondes après la prochaine image clé de la caméra.

### Configuration requise

- Linux sur **x86_64** ou **ARM64** (Raspberry Pi 3, 4 ou 5 avec un système 64 bits).
- Pour les fenêtres sur le bureau : un bureau avec X11, ou Wayland avec XWayland (GNOME, KDE Plasma, Xfce et la plupart des autres).
- **ffmpeg** : installé avec le paquet `.deb` ; sinon `sudo apt install ffmpeg` (Debian, Ubuntu, Raspberry Pi OS) ou le paquet de votre distribution.
- Pour les légendes sur le framebuffer : la police DejaVu (`fonts-dejavu-core`, normalement installée en même temps que ffmpeg).
- Pour le framebuffer : l’appartenance au groupe `video` (`sudo usermod -aG video $USER`, puis reconnectez-vous).
- Frigate accessible depuis l’ordinateur sur son port API 5000 (`http://`) ou 8971 (`https://`, avec un utilisateur Frigate) et, pour la vidéo en direct, sur le port RTSP 8554 de go2rtc (détails : [Configuration requise](#1-configuration-requise)).

### Installation

1. Installez le programme :
   - **Debian, Ubuntu, Raspberry Pi OS (64 bits), depuis le dépôt FrigatePiP** (recommandé — `sudo apt upgrade` met alors aussi FrigatePiP à jour) : ajoutez le dépôt une fois et installez le programme ; apt installe aussi ffmpeg :

     ```
     sudo wget -qO /usr/share/keyrings/frigate-pip.gpg https://maciejgornicz.github.io/frigate-pip/apt/frigate-pip.gpg
     echo "deb [signed-by=/usr/share/keyrings/frigate-pip.gpg] https://maciejgornicz.github.io/frigate-pip/apt stable main" | sudo tee /etc/apt/sources.list.d/frigate-pip.list
     sudo apt update
     sudo apt install frigate-pip
     ```

   - **Debian, Ubuntu, Raspberry Pi OS (64 bits), fichier de paquet unique** — téléchargez `frigate-pip_<version>_amd64.deb` (ou `frigate-pip_<version>_arm64.deb` pour un Raspberry Pi) depuis la page [Releases](https://github.com/maciejgornicz/frigate-pip/releases) et installez-le avec apt, qui installe aussi ffmpeg :

     ```
     sudo apt install ./frigate-pip_*.deb
     ```

   - **Autres distributions** — téléchargez `FrigatePiP-<version>-linux-x86_64.tar.gz` ou `FrigatePiP-<version>-linux-arm64.tar.gz`, décompressez l’archive et placez le programme dans un répertoire du PATH :

     ```
     tar xzf FrigatePiP-*-linux-*.tar.gz
     sudo install frigate-pip /usr/local/bin/
     ```

2. Lancez-le une première fois : il crée `~/.config/frigate-pip/config.ini`. Dans la section `[frigate]`, réglez `url` sur l’adresse de votre Frigate.
3. Vérifiez la connexion : `frigate-pip --test-connection`.
4. Lancez `frigate-pip`. Avec `--test <camera>` (ou les touches **1–9**), vous pouvez afficher une caméra immédiatement.

Le programme est un fichier unique, sans autres bibliothèques. Pour le mettre à jour, exécutez `sudo apt update && sudo apt upgrade` (dépôt), installez le paquet plus récent de la même façon ou remplacez le fichier ; pour le désinstaller, exécutez `sudo apt remove frigate-pip` (ou supprimez le fichier) et supprimez `~/.config/frigate-pip` et, si vous avez ajouté le dépôt, `/etc/apt/sources.list.d/frigate-pip.list` et `/usr/share/keyrings/frigate-pip.gpg`.

### Où apparaissent les fenêtres

Avec `--output auto` (par défaut), FrigatePiP ouvre des **fenêtres sur le bureau** lorsqu’il s’exécute sur un bureau (`DISPLAY` est défini), utilise le **framebuffer** sur une console Linux (`TERM=linux`) ou sans terminal (en tant que service), et le **terminal** dans les autres cas. Le mode **popups tmux** n’est utilisé que si vous le choisissez (`--output tmux` ou `output = tmux`). Dans un terminal, le mode d’image est choisi automatiquement : le protocole graphique Kitty s’il est disponible, sinon Sixel si le terminal indique le prendre en charge, sinon les demi-blocs. Ces deux réglages peuvent être définis dans le fichier de paramètres ou sur la ligne de commande.

### Fenêtres sur le bureau

Sur un bureau Linux, les fenêtres d’aperçu fonctionnent comme sous Windows : des fenêtres sans bordure au-dessus de toutes les autres, dans le coin choisi (`position`) de l’écran choisi (`monitor` : `primary` ou un numéro compté depuis la gauche), de la largeur `width` (multipliée par la mise à l’échelle du bureau). Elles nécessitent X11, ou Wayland avec XWayland — GNOME, KDE Plasma et la plupart des autres bureaux lancent XWayland pour ces programmes. Wayland seul ne suffit pas : il ne permet pas à un programme de placer une fenêtre dans un coin ni de la garder au premier plan.

- **Lancement :** menu des applications → **FrigatePiP** (installé avec apt), ou `frigate-pip` dans un terminal du bureau. Il s’exécute en arrière-plan sans terminal ; une seule copie s’exécute par utilisateur. Un second lancement (ou `frigate-pip --settings`) ouvre les paramètres de la copie en cours d’exécution.
- **Avec la session du bureau :** `frigate-pip --autostart on` (pour désactiver : `frigate-pip --autostart off`). Pour l’arrêter : `frigate-pip --quit`.
- **Icône dans la zone de notification :** avec le même menu que sous Windows (voir [Icône de la zone de notification](#4-icône-de-la-zone-de-notification)) ; sa couleur indique l’état : bleu — connecté, gris — non connecté, orange — en pause. Elle apparaît sous KDE Plasma, Xfce, Cinnamon, MATE et Budgie, ainsi que sous GNOME avec l’extension AppIndicator (incluse dans Ubuntu).
- **Souris :** comme sous Windows (voir [Actions de la souris](#actions-de-la-souris)) — × ou le bouton droit ferme, le haut-parleur coupe le son, le curseur en dessous règle le volume, ⟲ rejoue le début de l’événement, un glisser déplace la fenêtre, la molette la redimensionne, Ctrl + molette zoome sur l’image autour du pointeur, Ctrl + glisser déplace l’image zoomée, un clic l’épingle, un double clic ouvre la caméra dans Frigate, et une fenêtre sous la souris ne se ferme pas.
- **Options des fenêtres :** `topmost` (avec `false`, les fenêtres sont des fenêtres ordinaires que d’autres peuvent recouvrir), `sound` (le son de notification du bureau) et `skip_fullscreen` (rien tant que la fenêtre active est en plein écran) fonctionnent comme sous Windows.
- **Paramètres :** un clic sur l’icône (ou **Paramètres…** dans son menu) les ouvre dans le navigateur, avec les mêmes options, descriptions et le même test de connexion que sous Windows ; la page ne s’ouvre que sur cet ordinateur. Au premier lancement, elle s’ouvre d’elle-même. Ils sont enregistrés dans `~/.config/frigate-pip/config.ini` (décrit plus bas).

### Popups tmux

Dans ce mode, FrigatePiP n’a pas de fenêtre propre : il attend les détections en arrière-plan et, pour chacune d’elles, ouvre une popup [tmux](https://github.com/tmux/tmux) affichant la caméra par-dessus les volets du dernier client tmux que vous avez utilisé. Il nécessite tmux 3.2 ou une version ultérieure, ainsi que FrigatePiP 1.3.0 ou une version ultérieure (les versions plus anciennes ne connaissent pas `--output tmux` et utilisent sans prévenir le mode terminal).

**Les popups n’apparaissent que lorsque vous travaillez dans tmux.** tmux ne peut les dessiner que sur son propre écran : dans une fenêtre de terminal sans tmux — un shell ordinaire, une session SSH hors de tmux — rien ne s’affiche et les détections sont seulement consignées dans le journal. Lancer le processus d’arrière-plan ne suffit donc pas ; il vous faut aussi une session tmux dans laquelle travailler :

1. Lancez FrigatePiP une fois, dans une session tmux masquée : `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`.
2. Ouvrez tmux pour votre propre travail — `tmux` démarre une nouvelle session — et travaillez-y. Les popups apparaissent par-dessus cette fenêtre.

- Pour arrêter FrigatePiP : `tmux kill-session -t frigate-pip`. Pour utiliser ses touches (pause, affichage d’une caméra) : `tmux attach -t frigate-pip`.
- La popup apparaît dans le **coin** choisi (`position`) et sa taille suit la **largeur de la fenêtre** (`width`). Elle se ferme d’elle-même après le délai habituel (`duration`, `max_duration`), ou immédiatement avec **q**, **x** ou **Échap**. **m** coupe et rétablit son son. **r** rejoue le début de l’événement (depuis l’ouverture de la popup).
- tmux n’affiche qu’une popup à la fois : une détection sur une autre caméra remplace la popup ouverte (ou est ignorée si `replace = false`).
- Pour obtenir toutes les couleurs de l’image, tmux doit transmettre les couleurs 24 bits : ajoutez `set -as terminal-features ",xterm-256color:RGB"` à `~/.tmux.conf` (en indiquant la valeur de votre `$TERM` hors de tmux) et redémarrez tmux. FrigatePiP inscrit cette indication dans son journal lorsque les couleurs manquent.

### Touches

Lorsque FrigatePiP s’exécute dans un terminal ou sur une console :

| Touche | Effet |
|---|---|
| **q** (ou Ctrl+C) | Quitter. |
| **x** (ou Échap) | Fermer toutes les fenêtres d’aperçu. |
| **p** | Suspendre / reprendre les notifications. |
| **m** | Coupe / rétablit le son (avec `camera_audio = true`) dans toutes les fenêtres, y compris celles ouvertes ensuite ; la légende affiche alors *son coupé*. |
| **r** | Rejouer le début de l’événement dans toutes les fenêtres (voir [Actions de la souris](#actions-de-la-souris)) ; à nouveau : retour à la vidéo en direct. |
| **1**–**9** | Afficher la caméra 1 à 9 (dans l’ordre de la configuration de Frigate). |

### Ligne de commande

| Option | Effet |
|---|---|
| `--output auto\|window\|framebuffer\|terminal\|tmux` | Où dessiner les fenêtres (voir le tableau des modes ci-dessus). |
| `--graphics auto\|kitty\|sixel\|blocks` | Mode de dessin des images dans un terminal. |
| `--framebuffer /dev/fb0` | Périphérique framebuffer. |
| `--hwaccel auto\|on\|off` | Décodage vidéo sur la carte graphique (voir `hwaccel` plus bas). |
| `--test <camera>` | Afficher une caméra juste après le démarrage. |
| `--test-connection` | Vérifier la connexion à Frigate (comme **Tester la connexion** sous Windows), puis quitter. |
| `--list` | Affiche les caméras avec leurs objets et zones — les noms pour `cameras`, `labels` et `zones` dans `[trigger]` — puis quitte. |
| `--autostart on\|off` | Lance les fenêtres sur le bureau avec la session du bureau (une entrée dans `~/.config/autostart`), ou arrête de le faire. |
| `--quit` | Arrête les fenêtres sur le bureau en cours d’exécution. |
| `--settings` | Ouvre les paramètres des fenêtres sur le bureau en cours d’exécution (ou les lance avec les paramètres ouverts). |
| `--version`, `--help` | Version, aide succincte. |

### Paramètres

La version Linux utilise le même `config.ini` (dans `~/.config/frigate-pip/` ou `$XDG_CONFIG_HOME/frigate-pip/`) que la version Windows — voir [Fichier de paramètres](#7-fichier-de-paramètres). Les modifications sont appliquées dans les deux secondes qui suivent l’enregistrement du fichier. Les paramètres suivants sont utilisés : tout ce qui se trouve dans `[app]` (langue), `[frigate]`, `[mqtt]` et `[trigger]`, ainsi que, dans `[display]` : `monitor`, `position`, `margin`, `width`, `duration`, `max_duration`, `overlay`, `max_popups`, `replace`, `topmost`, `sound`, `skip_fullscreen` ; dans `[video]` : `live`, `stream`, `auto_main_width`, `prebuffer` (pas pour les popups tmux), `camera_audio` ; et les sections `[camera.<name>]`. Les paramètres propres à Windows (`gpu_warm`, `check_updates`) sont ignorés. `frigate-pip --list` affiche les noms de vos caméras, objets et zones pour les filtres de `[trigger]`.

Paramètres supplémentaires de la section `[console]` :

| Clé | Par défaut | Signification |
|---|---|---|
| `output` | `auto` | `auto`, `window`, `framebuffer`, `terminal` ou `tmux`. |
| `graphics` | `auto` | `auto`, `kitty`, `sixel` ou `blocks` (images dans un terminal). |
| `framebuffer` | `/dev/fb0` | Périphérique framebuffer. |
| `ffmpeg` | `ffmpeg` | Programme ffmpeg (nom ou chemin complet). |
| `hwaccel` | `auto` | Décodage vidéo sur la carte graphique (VAAPI, VDPAU, NVIDIA… ce que ffmpeg trouve ; sinon sur le processeur) : `auto` = pour le flux main, où il économise le plus (la première image d’une telle fenêtre peut arriver environ 0,2 s plus tard ; le préchargement le masque), `on` = toujours, `off` = jamais. |
| `font` | *(DejaVu Sans Bold)* | Police TrueType des légendes sur le framebuffer. |

La langue suit les paramètres régionaux du système (`LANG`), sauf si `language` est défini dans `[app]`. Le son de la caméra est lu via PulseAudio/PipeWire lorsqu’ils sont disponibles, sinon via ALSA.

### Démarrage automatique (systemd)

Pour un écran qui affiche la console (p. ex. un Raspberry Pi sans bureau), FrigatePiP peut démarrer comme service au démarrage de l’ordinateur.

**Installé avec apt (dépôt ou fichier `.deb`) :** le service est inclus — il suffit de l’activer pour votre utilisateur (remplacez `pi` par votre nom d’utilisateur) : `sudo systemctl enable --now frigate-pip@pi`. Son journal : `journalctl -u frigate-pip@pi`.

**Installé depuis l’archive :** créez `/etc/systemd/system/frigate-pip.service` (remplacez `pi` par votre nom d’utilisateur) :

```ini
[Unit]
Description=FrigatePiP
After=network-online.target
Wants=network-online.target

[Service]
User=pi
SupplementaryGroups=video audio
ExecStart=/usr/local/bin/frigate-pip --output framebuffer
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

Ensuite : `sudo systemctl enable --now frigate-pip`. Le journal est écrit dans le journal systemd (`journalctl -u frigate-pip`) et dans `~/.config/frigate-pip/frigate-pip.log`. Pour que le curseur clignotant de la console ne gêne pas les fenêtres, ajoutez `vt.global_cursor_default=0` à la ligne de commande du noyau (sur un Raspberry Pi : `/boot/firmware/cmdline.txt`).

### Dépannage sous Linux

- **« cannot start ffmpeg »** — installez ffmpeg (`sudo apt install ffmpeg`) ou définissez `ffmpeg` dans `[console]`.
- **« /dev/fb0: Permission denied »** — ajoutez l’utilisateur au groupe `video` et reconnectez-vous (pour le service : `SupplementaryGroups=video`).
- **Pas de légende sur le framebuffer** — installez `fonts-dejavu-core` ou définissez `font` dans `[console]`.
- **L’image est pixélisée dans le terminal** — c’est le mode demi-blocs ; utilisez un terminal prenant en charge le protocole graphique Kitty ou Sixel (voir ci-dessus) ou définissez `graphics`.
- **Aucune popup tmux n’apparaît** — les popups ne s’affichent que lorsque vous travaillez dans tmux, pas dans une fenêtre de terminal ordinaire (voir [Popups tmux](#popups-tmux)). Vérifiez qu’un client tmux est attaché (`tmux list-clients` l’affiche), que `frigate-pip --version` indique 1.3.0 ou une version ultérieure, que tmux est en version 3.2 ou ultérieure (`tmux -V`) et que le processus d’arrière-plan s’exécute (`tmux ls` affiche la session `frigate-pip`) ; le journal indique pourquoi une popup n’a pas été affichée.
- **Couleurs pâles dans les popups tmux** — activez les couleurs 24 bits dans tmux (voir [Popups tmux](#popups-tmux)).
- **Pas de son** — le son de la caméra nécessite PulseAudio/PipeWire ou ALSA ; l’utilisateur du service doit appartenir au groupe `audio`.
- **Pour un rapport détaillé**, démarrez FrigatePiP avec `FRIGATEPIP_FFMPEG_LOG=/tmp/ffmpeg.log frigate-pip` et joignez à votre ticket la fin de ce fichier ainsi que celle du journal.
- **Aucune fenêtre sur le bureau** — elles nécessitent X11 ou XWayland : `echo $DISPLAY` doit afficher quelque chose comme `:0`. Sinon, FrigatePiP utilise les modes console ; le journal en indique la raison (`No desktop windows`).
- **FrigatePiP indique qu’il est déjà lancé** — une seule copie s’exécute par utilisateur, par exemple celle lancée avec la session du bureau ; `frigate-pip --quit` l’arrête.
- **Pas d’icône dans la zone de notification sous GNOME** — GNOME n’affiche ces icônes qu’avec l’extension « AppIndicator and KStatusNotifierItem Support » (paquet `gnome-shell-extension-appindicator` ; incluse dans Ubuntu). Sans elle, relancez FrigatePiP depuis le menu des applications pour ouvrir les paramètres.
