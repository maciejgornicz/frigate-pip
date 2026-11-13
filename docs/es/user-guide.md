# FrigatePiP — Guía del usuario

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

FrigatePiP es una pequeña aplicación de la bandeja del sistema de Windows para [Frigate NVR](https://frigate.video). Cuando Frigate detecta una persona, un automóvil o cualquier otro objeto, FrigatePiP muestra de inmediato una pequeña ventana flotante (picture-in-picture) con el video en vivo de esa cámara en una esquina de la pantalla. Cuando ya no hay movimiento, la ventana se cierra sola.

Las secciones 1–11 describen la aplicación para Windows. FrigatePiP también funciona en Linux, en el escritorio y en una consola sin escritorio; esa versión se describe por separado en la sección 12, [Versión para Linux](#12-versión-para-linux).

## Contenido

1. [Requisitos](#1-requisitos)
2. [Instalación](#2-instalación)
3. [Primer inicio](#3-primer-inicio)
4. [El icono de la bandeja](#4-el-icono-de-la-bandeja)
5. [Ventanas de vista previa](#5-ventanas-de-vista-previa)
6. [La ventana de configuración](#6-la-ventana-de-configuración)
7. [El archivo de configuración](#7-el-archivo-de-configuración)
8. [Línea de comandos](#8-línea-de-comandos)
9. [Idioma](#9-idioma)
10. [Archivos, privacidad y recursos](#10-archivos-privacidad-y-recursos)
11. [Solución de problemas](#11-solución-de-problemas)
12. [Versión para Linux](#12-versión-para-linux)

---

## 1. Requisitos

- **Windows 10 (versión 1809 o posterior) o Windows 11**, de 64 bits.
- **Frigate** accesible desde su equipo a través de la red. Se recomienda Frigate 0.14 o posterior; los modos *Review* requieren la versión 0.14 o posterior.
- La **API** de Frigate debe ser accesible desde el equipo: el **puerto 5000** sin autenticación (`http://`, sin inicio de sesión) o el **puerto 8971** autenticado (`https://`, con un usuario de Frigate; basta con el rol *viewer*; créelo en *Configuración → Usuarios* en Frigate). En Docker, publique el puerto, p. ej., `- "5000:5000"` o `- "8971:8971"`. También funciona un proxy inverso delante de Frigate.
- **Complemento de Frigate para Home Assistant:** el puerto 5000 está desactivado de forma predeterminada; actívelo en la configuración de *Red* del complemento. También puede usar el puerto 8971 con un usuario de Frigate.
- Para un video en vivo fluido: el **servidor RTSP de go2rtc integrado en Frigate, puerto 8554** (`- "8554:8554"`). Sin él, puede seguir usando MJPEG o instantáneas (consulte [Imagen en vivo](#imagen-en-vivo)).
- Opcional: un **bróker MQTT** usado por Frigate, si prefiere MQTT al WebSocket de Frigate.
- Para cámaras H.265 (HEVC), Windows necesita un decodificador HEVC. La mayoría de los equipos con un controlador gráfico reciente lo tienen; de lo contrario, instale *Extensiones de video HEVC* desde Microsoft Store o use un flujo sub H.264. Si no hay ningún decodificador disponible, FrigatePiP muestra automáticamente MJPEG de Frigate en su lugar.
- **Las ediciones N de Windows** (por ejemplo, Windows 11 Pro N) no incluyen Media Foundation hasta que se agrega la característica opcional de Windows *Media Feature Pack*. FrigatePiP funciona igualmente sin ella, pero el video en vivo llega como MJPEG de Frigate (unos 5 fotogramas por segundo) y el audio AAC de la cámara no está disponible.

## 2. Instalación

### Con el instalador (recomendado)

1. Descargue `FrigatePiP-<version>-installer.exe` desde la página [Releases](https://github.com/maciejgornicz/frigate-pip/releases).
2. Ejecútelo. El instalador no necesita derechos de administrador; instala solo para el usuario actual, en `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Elija si FrigatePiP debe **iniciarse con Windows** (activado de forma predeterminada) y si se crea un acceso directo en el escritorio.
4. Al final, deje marcada la casilla **Ejecutar FrigatePiP**. Aparece el icono en la bandeja y, en el primer inicio, se abre la ventana de configuración.

El instalador y la aplicación no están firmados con un certificado de firma de código. Windows SmartScreen puede mostrar *«Windows protegió su PC»*; haga clic en **Más información → Ejecutar de todas formas**. Puede verificar la suma de comprobación SHA-256 publicada con cada versión.

### Versión portátil (sin instalación)

Descargue `FrigatePiP-<version>-portable.exe` desde la página Releases y ejecútelo desde cualquier carpeta. La configuración se guarda en el mismo lugar que con el instalador (consulte [Archivos](#10-archivos-privacidad-y-recursos)). Use **Iniciar FrigatePiP con Windows** en la configuración para que se inicie automáticamente.

### Actualización

FrigatePiP busca una versión nueva una vez al día (**Buscar actualizaciones** en la configuración). Cuando hay una disponible, aparece una notificación y tanto el menú de la bandeja como la ventana de configuración ofrecen **Actualizar a …**: FrigatePiP descarga el instalador desde GitHub, comprueba su suma de comprobación SHA-256 y lo ejecuta; la actualización tarda unos segundos y conserva su configuración. **Buscar ahora** en la configuración busca una versión nueva al instante. También puede ejecutar usted mismo el instalador de una versión nueva. La versión portátil solo abre la página de la versión: descargue el nuevo exe y reemplace el anterior.

### Desinstalación

**Configuración → Aplicaciones → Aplicaciones instaladas → FrigatePiP → Desinstalar** (o *Agregar o quitar programas*). El desinstalador cierra FrigatePiP y elimina el programa, los accesos directos y la entrada de inicio automático. Su configuración y el registro en `%APPDATA%\FrigatePiP` se conservan; elimine esa carpeta para quitarlos también.

## 3. Primer inicio

1. En el primer inicio, la ventana de configuración se abre en la pestaña **Conexión**.
2. Escriba la **Dirección de Frigate**, p. ej., `http://192.168.1.10:5000`.
3. Haga clic en **Probar conexión**. FrigatePiP comprueba, sin guardar nada:
   - la API de Frigate y su versión,
   - la lista de cámaras,
   - el origen de eventos (WebSocket o MQTT),
   - el video RTSP de go2rtc, incluido el códec y si la cámara tiene audio.

   Cada línea empieza con ✔ (funciona) o ✖ (problema, con una sugerencia).
4. Haga clic en **OK**. A partir de ese momento, FrigatePiP espera detecciones en segundo plano.
5. Para ver cómo es una ventana, haga clic con el botón derecho en el icono de la bandeja y elija **Mostrar cámara → (una cámara)**, o marque **Mostrar en vivo** en la parte inferior de la ventana de configuración.

## 4. El icono de la bandeja

El icono del área de notificación muestra el estado:

| Color | Significado |
|---|---|
| Azul | Conectado a Frigate, esperando detecciones |
| Gris | Sin conexión (FrigatePiP vuelve a intentarlo automáticamente) |
| Ámbar | Las notificaciones están en pausa |

Pase el puntero sobre el icono para ver el estado (por ejemplo, *Conectado: http://192.168.1.10:5000*).

- **Clic izquierdo**: abre la ventana de configuración.
- **Clic derecho**: abre el menú.

| Elemento del menú | Qué hace |
|---|---|
| *(línea de estado)* | Estado de la conexión o tiempo de pausa restante (informativo). |
| **Actualizar a …** | Aparece cuando hay una versión nueva disponible: la descarga y la instala (consulte [Actualización](#actualización)). |
| **Configuración…** | Abre la ventana de configuración. |
| **Mostrar cámara** | Abre de inmediato una ventana de vista previa de la cámara elegida, incluso con las notificaciones en pausa. Sirve para comprobar la imagen o simplemente para mirar. |
| **Pausar notificaciones** | **Durante 15 minutos**, **Durante 1 hora** o **Hasta reanudar**. Durante la pausa, las detecciones no abren ventanas. Mientras hay una pausa activa, el elemento muestra **En pausa**, y **Reanudar ahora** la termina. |
| **Cerrar ventanas de vista previa** | Cierra todas las ventanas de vista previa abiertas, incluidas las fijadas. |
| **Abrir Frigate** | Abre la interfaz web de Frigate en el navegador. |
| **Ayuda** | Abre esta guía. |
| **Mostrar registro** | Abre el archivo de registro en el Bloc de notas (útil cuando algo no funciona). |
| **Archivo de configuración (avanzado)…** | Abre `config.ini` en el Bloc de notas. Los cambios guardados se aplican de inmediato. |
| **Salir** | Cierra FrigatePiP. |

## 5. Ventanas de vista previa

Cuando una detección coincide con su configuración, aparece una ventana de vista previa en la esquina de la pantalla elegida:

1. La ventana aparece al instante, a los pocos milisegundos del mensaje de Frigate.
2. Primero muestra la última instantánea de la cámara en Frigate y luego cambia al video en vivo en cuanto el flujo entrega una imagen (normalmente en menos de un segundo; casi al instante con el [búfer previo](#búfer-previo)).
3. El texto sobre la imagen (si está activado) muestra el nombre de la cámara, el objeto (por ejemplo, *persona*), la subetiqueta (por ejemplo, un rostro o una placa de matrícula reconocidos), la confianza y la hora. Un **punto rojo** indica que la imagen es en vivo.

Las ventanas de vista previa nunca toman el foco del teclado, por lo que no interrumpen la escritura ni los juegos.

### Acciones con el mouse

| Acción | Efecto |
|---|---|
| **Arrastrar** | Mueve la ventana. Se queda donde la coloque hasta que se cierre. |
| **Clic** | **Fija** la ventana: permanece abierta hasta que usted la cierre (el texto muestra *fijada*). Haga clic de nuevo para dejar de fijarla. |
| **Rueda del mouse** | Agranda o reduce la ventana. |
| **Ctrl + rueda del mouse** | Acerca (hasta 8×) o aleja la imagen alrededor del puntero; el texto muestra el zoom, p. ej. *2.0×*. El detalle depende del flujo: el flujo main (`stream = main`) es el que más tiene. |
| **Ctrl + arrastrar** | Con la imagen acercada: mueve la imagen para ver otra parte de ella. Sin zoom se mueve la ventana, como siempre. |
| **×** (esquina superior derecha) | Cierra la ventana al instante. El botón aparece al señalar la ventana con el mouse. |
| **Altavoz** (a la izquierda de ×) | Silencia la ventana; otro clic vuelve a activar el sonido. El botón aparece al señalar una ventana que reproduce el audio de la cámara; mientras el sonido está desactivado, el altavoz tachado permanece visible. |
| **Control de volumen** (bajo el altavoz) | Ajusta el volumen de esta ventana: arrástrelo o haga clic en él. Aparece junto con el altavoz; subir el volumen también reactiva el sonido silenciado. En Linux, el nuevo volumen se oye justo después de soltar. |
| **Repetición** ⟲ (a la izquierda de los demás botones) | Reproduce otra vez el inicio del evento a velocidad normal, sin sonido; el texto muestra *Repetición −12 s*, es decir, cuánto va la imagen por detrás del video en vivo. Con el [búfer previo](#búfer-previo) la repetición empieza 5 s antes de que se abriera la ventana; sin él, al abrirse la ventana. Haga clic de nuevo para volver al instante al video en vivo; al terminar la repetición vuelve solo. El botón aparece al señalar una ventana con video en vivo; durante la repetición la ventana no se cierra. |
| **Clic derecho** | Cierra la ventana. |
| **Doble clic** | Abre esta cámara en la interfaz web de Frigate. |
| **Pasar el puntero** | Mientras el puntero del mouse está sobre la ventana, esta no se cierra. Después de retirarlo, permanece abierta al menos 3 segundos más. |

### ¿Cuándo se cierra una ventana?

- Cuando pasan los segundos indicados en **Tras el último movimiento** desde que el objeto (o cualquier otro objeto coincidente de esa cámara) se movió por última vez.
- Como muy tarde, tras los segundos indicados en **máximo**, aunque el objeto siga moviéndose.
- Nunca mientras está fijada o mientras el puntero del mouse está sobre ella.
- Nunca durante una repetición; después sigue abierta al menos 5 segundos más.

### Más de una cámara

Cada cámara tiene como máximo una ventana. Las nuevas detecciones en una cámara que ya tiene ventana solo la prolongan y actualizan el texto. Las ventanas de distintas cámaras se apilan a partir de la esquina elegida. **Número máx. de ventanas** limita cuántas pueden estar abiertas a la vez; **Si todas están en uso** decide si una cámara nueva reemplaza la ventana más antigua (en el mismo lugar de la pantalla; las ventanas fijadas nunca se reemplazan) o se omite.

## 6. La ventana de configuración

Ábrala con un clic izquierdo en el icono de la bandeja o con **Configuración…** en el menú. **Cada opción tiene una descripción: pase el puntero del mouse por encima.** Presione **F1** o haga clic en **Ayuda** para abrir esta guía.

La mayoría de los cambios se aplican **de inmediato** a las ventanas de vista previa abiertas, así que puede ver el resultado antes de guardar. **OK** guarda y cierra, **Aplicar** guarda y mantiene la ventana abierta, y **Cancelar** (o cerrar la ventana) descarta los cambios no guardados.

### 6.1 Conexión

| Opción | Descripción |
|---|---|
| **Dirección de Frigate** | Dirección del servidor Frigate con el puerto: `http://192.168.1.10:5000` (puerto 5000, sin inicio de sesión) o `https://192.168.1.10:8971` (puerto 8971, con el usuario de Frigate de abajo). También sirve la dirección de un proxy inverso. |
| **Usuario / contraseña de Frigate** | Solo para el puerto 8971 o un proxy inverso que pida iniciar sesión en Frigate: un usuario de Frigate (*Configuración → Usuarios* en Frigate); basta con el rol *viewer*. Déjelo vacío para el puerto 5000. FrigatePiP inicia sesión, guarda el token que le da Frigate y vuelve a iniciar sesión cuando caduca. La contraseña se guarda en `config.ini`, como la de MQTT. |
| **Origen de eventos** | **WebSocket de Frigate (recomendado)** — FrigatePiP se conecta directamente a Frigate; no se necesita nada más. **MQTT** — las detecciones se leen de su bróker MQTT (el configurado en Frigate). Ambos dan el mismo resultado; el WebSocket suele ser algo más rápido. |
| **Servidor MQTT**, **puerto** | Nombre de host o dirección IP y puerto (normalmente 1883) del bróker MQTT. Solo se usa con el origen MQTT. |
| **Usuario / contraseña** | Inicio de sesión de MQTT. Déjelo en blanco si el bróker permite el acceso anónimo. |
| **Prefijo de temas MQTT** | `mqtt.topic_prefix` de la configuración de Frigate, normalmente `frigate`. |
| **Dirección RTSP (go2rtc)** | Plantilla de la dirección del video en vivo. `{host}` se sustituye por el host de la dirección de Frigate y `{stream}`, por el nombre del flujo de go2rtc. El valor predeterminado `rtsp://{host}:8554/{stream}` sirve para una instalación estándar de Frigate. Si go2rtc requiere inicio de sesión, use `rtsp://user:password@{host}:8554/{stream}`. |
| **Probar conexión** | Comprueba los valores indicados arriba (Frigate, cámaras, eventos, video y audio RTSP) sin guardarlos. |
| **Idioma** | Idioma de FrigatePiP. **Automático (Windows)** usa el idioma para mostrar de Windows; si ese idioma no está disponible, se usa el inglés. Consulte [Idioma](#9-idioma). |
| **Iniciar FrigatePiP con Windows** | Inicia FrigatePiP automáticamente al iniciar sesión en Windows. |
| **Buscar actualizaciones** | Una vez al día FrigatePiP consulta en GitHub si hay una versión nueva y, si la hay, muestra una notificación y ofrece la actualización. No se envía ningún dato sobre usted ni sus cámaras. |
| **Buscar ahora** / **Actualizar a …** | Busca una versión nueva al instante; si la encuentra, el botón la descarga y la instala (la configuración se conserva). El texto de al lado muestra el resultado. |

### 6.2 Detección

| Opción | Descripción |
|---|---|
| **Mostrar ventana para** | Qué mensajes de Frigate abren una ventana: **Cada objeto detectado (lo más rápido)** — cada objeto rastreado por Frigate; **Solo alertas de Review** — solo los elementos de Review con gravedad *alert*; **Alertas y detecciones de Review** — todos los elementos de Review. Los elementos de Review siguen las reglas de alertas y detecciones (etiquetas y zonas requeridas) definidas en la configuración `review` de Frigate y llegan un poco más tarde que los eventos de objetos. |
| **Cámaras** | Cámaras que pueden abrir una ventana. Las cámaras no marcadas se ignoran. Con todas las cámaras marcadas, también se incluyen las que se agreguen a Frigate más adelante. |
| **Objetos** | Tipos de objeto (etiquetas) que pueden abrir una ventana, p. ej., persona, automóvil, perro. La lista muestra los objetos que rastrean sus cámaras. Con todos marcados, cuenta cualquier tipo de objeto. |
| **Zonas requeridas** | Si marca zonas, la ventana solo se abre cuando el objeto entra en una de ellas. Si no hay ninguna marcada, cuenta cualquier punto de la imagen. |
| **Ignorar objetos que no se mueven** | Los objetos que Frigate marca como inmóviles (por ejemplo, un vehículo estacionado) no abren ventanas ni las mantienen abiertas. Se aplica a *Cada objeto detectado*. |
| **Confianza mínima** | Las detecciones con una confianza menor se ignoran. 0% significa que deciden los umbrales propios de Frigate. Se aplica a *Cada objeto detectado*. |

Los falsos positivos de Frigate siempre se ignoran. Cada objeto abre una ventana solo una vez; cuando vuelve a moverse, solo prolonga la ventana abierta.

Los **eventos manuales** también abren una ventana: los eventos creados con la API de Frigate (`POST /api/events/<cámara>/<etiqueta>/create`, por ejemplo desde una automatización de Home Assistant cuando suena el timbre) y los elementos de una cámara LPR dedicada. El título de la ventana muestra la etiqueta del evento. Funciona con **Cada objeto detectado (lo más rápido)** y **Alertas y detecciones de Review**, y con **Solo alertas de Review** cuando Frigate convierte el evento en una alerta (su etiqueta está entre las etiquetas de alerta de Review, por defecto `person` y `car`). Para probarlo en PowerShell: `Invoke-RestMethod -Method Post -ContentType application/json -Body '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`; en Linux: `curl -X POST -H "Content-Type: application/json" -d '{"duration": 10}' http://<frigate>:5000/api/events/<camera>/doorbell/create`. Los filtros de cámaras, objetos y zonas se aplican igual que a otras detecciones (un filtro de objetos `person, car` oculta un evento `doorbell`); la confianza mínima no. Frigate solo crea el elemento de Review al que reacciona FrigatePiP si la cámara graba (`record` activado). Un evento manual creado mientras el elemento de Review de la cámara sigue abierto (hasta el tiempo de corte de la alerta, 40 s por defecto tras el evento anterior) se añade a ese elemento, y Frigate solo lo anuncia cuando se mueve un objeto en la cámara; al probar, espere aproximadamente un minuto entre dos eventos.

### 6.3 Ventana de vista previa

| Opción | Descripción |
|---|---|
| **Monitor** | Monitor en el que aparecen las ventanas: **Principal** o un monitor concreto (numerados desde la izquierda, con su resolución). |
| **Esquina de la pantalla** | Esquina en la que aparecen las ventanas. Las demás ventanas se apilan a partir de esta esquina. |
| **Ancho de la ventana** | Ancho en píxeles (con una escala del 100%; FrigatePiP lo ajusta a la escala del monitor). La altura se ajusta a la relación de aspecto de la cámara. También puede cambiar el tamaño de una sola ventana con la rueda del mouse. |
| **Distancia al borde** | Separación entre las ventanas y los bordes de la pantalla (y entre las ventanas). |
| **Tras el último movimiento** | Cuántos segundos permanece abierta una ventana después del último movimiento. |
| **máximo** | Tiempo máximo en segundos que se muestra una ventana, aunque el objeto siga moviéndose (salvo que esté fijada). |
| **Número máx. de ventanas** | Cuántas ventanas (cámaras) pueden estar abiertas a la vez (1–8). |
| **Si todas están en uso** | **Reemplazar la ventana más antigua (en el mismo lugar)** — la nueva cámara ocupa el lugar de la ventana no fijada más antigua; **Omitir el nuevo evento** — la nueva detección se ignora. |
| **Siempre en primer plano** | Mantiene las ventanas por encima de todas las demás. |
| **Texto sobre la imagen** | Muestra la cámara, el objeto, la confianza y la hora sobre la imagen. |
| **Sonido de notificación** | Reproduce el sonido *Asterisco* de Windows cuando aparece una ventana nueva. No es el audio de la cámara (consulte [Audio de la cámara](#audio-de-la-cámara)). |
| **No mostrar durante juegos o presentaciones a pantalla completa** | No aparecen ventanas mientras se ejecuta un juego, un video o una presentación a pantalla completa (las detecciones se omiten, no se retrasan). |

### 6.4 Video y audio

#### Imagen en vivo

| Opción | Descripción |
|---|---|
| **RTSP de go2rtc** (predeterminado) | Video fluido a la velocidad de fotogramas de la cámara, con decodificación por hardware (H.264 y H.265) y bajo retardo. Requiere el puerto 8554. |
| **MJPEG de Frigate** | Unos 5 fotogramas por segundo a la resolución de detección, a través de la API de Frigate. Úselo cuando RTSP no esté disponible. |
| **Solo instantáneas actualizadas** | Solo las instantáneas de Frigate, actualizadas periódicamente. La opción más ligera. |

#### Flujo

| Opción | Descripción |
|---|---|
| **Automático (según el tamaño de la ventana)** | El flujo **sub** ligero para ventanas normales y el flujo **main** de resolución completa cuando la ventana es más ancha que el valor de **Main si la ventana supera** (en píxeles físicos). |
| **Sub (ligero)** | Siempre el flujo sub. |
| **Main (resolución completa)** | Siempre el flujo main. |

FrigatePiP encuentra los flujos de go2rtc de cada cámara en la configuración de Frigate (`live.streams`, `live.stream_name` o flujos de go2rtc llamados `<camera>` y `<camera>_sub`). Puede definirlos usted mismo en el [archivo de configuración](#configuración-por-cámara).

#### Audio de la cámara

**Audio de la cámara** reproduce el sonido de la cámara (AAC o G.711 del flujo RTSP) en cada ventana abierta. Funciona por separado, por lo que nunca retrasa la ventana ni el video. El volumen se puede ajustar en el Mezclador de volumen de Windows (*FrigatePiP*). Requiere **RTSP de go2rtc**. Para silenciar una sola ventana, haga clic en el botón del altavoz de su esquina superior derecha (consulte [Acciones con el mouse](#acciones-con-el-mouse)). **Volumen** define con qué volumen empieza una ventana nueva (0–100 %, `camera_volume`), por ejemplo para atenuar el ruido del viento; cada ventana tiene además su propio control. **Filtro de viento** (`wind_filter`) elimina el retumbar del viento en el micrófono de la cámara y deja las voces a su volumen: con el aire en calma solo quita lo que está por debajo de 80 Hz, durante una ráfaga todo hasta 500 Hz, por lo que la voz suena más fina en ese momento. No puede arreglar un micrófono que el viento satura: ahí ayuda un trozo de espuma sobre la abertura del micrófono.

#### Búfer previo

**Búfer previo** mantiene una conexión con el flujo sub de cada cámara, de modo que el video en vivo empieza al instante (unos 0,15 s) en lugar de esperar al siguiente fotograma clave (hasta 1 s aproximadamente, según el intervalo de fotogramas clave de la cámara). Consumo: unos 0,1–0,5 Mbit/s de tráfico de la red local por cámara, de forma continua. Además guarda en memoria los últimos 30 s de cada cámara (lo que ocupan 30 s de su flujo sub: unos 0,2–2 MB por cámara), para que la [repetición](#acciones-con-el-mouse) pueda empezar 5 s antes del evento.

#### GPU preparada

**GPU preparada** mantiene inicializado el dispositivo gráfico, de modo que el video empieza unos 130 ms antes. Consumo: aproximadamente el 1% de un núcleo de CPU en reposo (hilo del controlador gráfico).

### 6.5 Parte inferior de la ventana

| Control | Descripción |
|---|---|
| **Vista previa:** cámara | Cámara usada para la vista previa en vivo de la configuración. |
| **Mostrar en vivo** | Abre una ventana de vista previa real de esa cámara. Refleja sus cambios (esquina, tamaño, texto, flujo, audio…) al instante, antes de guardarlos. Está fijada y no cuenta para el límite de ventanas. |
| **OK** / **Cancelar** / **Aplicar** | Guardar y cerrar / descartar / guardar y mantener la ventana abierta. |
| **Ayuda** | Abre esta guía (también **F1**). |

## 7. El archivo de configuración

Toda la configuración se guarda en `%APPDATA%\FrigatePiP\config.ini` (un archivo de texto sin formato). La ventana de configuración escribe todos los valores; el archivo solo es necesario para la configuración por cámara. **Archivo de configuración (avanzado)…** en el menú de la bandeja lo abre en el Bloc de notas. Los cambios se aplican automáticamente al guardar el archivo. Los comentarios empiezan con `;`. Los valores de sí/no aceptan `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Sección | Clave | Predeterminado | Significado |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (idioma de Windows) o `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Buscar una versión nueva una vez al día. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Dirección de Frigate. |
| | `user`, `password` | *(vacío)* | Inicio de sesión en Frigate para el puerto 8971 (vacío para el puerto 5000). |
| | `source` | `websocket` | `websocket` o `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | Plantilla de la dirección RTSP. |
| `[mqtt]` | `host` | *(vacío)* | Bróker MQTT (solo con `source = mqtt`). |
| | `port` | `1883` | Puerto del bróker. |
| | `user`, `password` | *(vacío)* | Inicio de sesión. |
| | `prefix` | `frigate` | Prefijo de temas. |
| `[trigger]` | `mode` | `events` | `events` (cada objeto), `alerts` (alertas de Review) o `reviews` (alertas y detecciones). |
| | `cameras` | *(vacío = todas)* | Nombres de cámaras separados por comas, p. ej., `front_door, garden`. |
| | `labels` | *(vacío = todos)* | Objetos separados por comas, p. ej., `person, car`. |
| | `zones` | *(vacío = cualquier lugar)* | Zonas requeridas. |
| | `ignore_stationary` | `true` | Ignorar objetos inmóviles. |
| | `min_score` | `0` | Confianza mínima de 0.0 a 1.0. |
| `[display]` | `monitor` | `primary` | `primary` o número de monitor desde la izquierda (1, 2, 3…). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Distancia al borde de la pantalla en píxeles. |
| | `width` | `480` | Ancho de la ventana en píxeles. |
| | `duration` | `15` | Segundos tras el último movimiento. |
| | `max_duration` | `120` | Máximo de segundos que se muestra una ventana. |
| | `topmost` | `true` | Siempre en primer plano. |
| | `sound` | `false` | Sonido de notificación. |
| | `overlay` | `true` | Texto sobre la imagen. |
| | `max_popups` | `4` | Número máx. de ventanas. |
| | `replace` | `true` | `true` = reemplazar la ventana más antigua, `false` = omitir el nuevo evento. |
| | `skip_fullscreen` | `false` | No mostrar durante aplicaciones a pantalla completa. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` o `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` o `main`. |
| | `auto_main_width` | `800` | Ancho de ventana (px) a partir del cual `auto` usa el flujo main. |
| | `prebuffer` | `false` | Búfer previo. |
| | `gpu_warm` | `false` | GPU preparada. |
| | `camera_audio` | `false` | Audio de la cámara. |
| | `camera_volume` | `100` | Volumen de una ventana nueva, 0–100 %. |
| | `wind_filter` | `false` | Filtro de viento: elimina el retumbar del viento del audio de la cámara. |

### Configuración por cámara

Agregue una sección `[camera.<name>]`, donde `<name>` es el nombre de la cámara en Frigate:

```ini
[camera.front_door]
; Nombre que se muestra en el texto sobre la imagen, el menú y la configuración
name = Front door
; Nombres de los flujos de go2rtc
main = front_door
sub = front_door_sub
```

## 8. Línea de comandos

| Comando | Efecto |
|---|---|
| `frigate-pip.exe` | Inicia FrigatePiP (solo se ejecuta una copia a la vez). |
| `frigate-pip.exe --settings` | Abre la ventana de configuración (inicia FrigatePiP si no está en ejecución). |
| `frigate-pip.exe --test <camera>` | Muestra de inmediato una ventana de vista previa de la cámara, p. ej., `--test front_door`. Puede crear un acceso directo de Windows con una tecla de método abreviado para su cámara favorita. |
| `frigate-pip.exe --quit` | Cierra la instancia de FrigatePiP en ejecución. |

Si FrigatePiP ya está en ejecución, el comando se transmite a esa instancia.

## 9. Idioma

FrigatePiP está disponible en inglés, polaco, alemán, español, portugués y francés. Con **Automático (Windows)** usa el idioma para mostrar de Windows (*Configuración → Hora e idioma → Idioma y región → Idioma para mostrar de Windows*), o el inglés si ese idioma no está disponible. Puede elegir el idioma usted mismo en **Conexión → Idioma**; la ventana de configuración cambia de inmediato.

## 10. Archivos, privacidad y recursos

| Qué | Dónde |
|---|---|
| Programa | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (instalador) |
| Configuración | `%APPDATA%\FrigatePiP\config.ini` |
| Registro | `%APPDATA%\FrigatePiP\frigate-pip.log` (hasta 512 KB; el anterior se conserva como `frigate-pip.old.log`) |
| Inicio automático | Valor `FrigatePiP` del Registro de Windows en `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Privacidad:** FrigatePiP se conecta solo a las direcciones que usted indica (Frigate, go2rtc y el bróker MQTT) y, salvo que desactive **Buscar actualizaciones**, una vez al día a GitHub para buscar una versión nueva. No tiene telemetría, ni cuentas, ni servicios en la nube. Las imágenes solo se muestran y nunca se guardan. Los enlaces de **Ayuda** abren esta guía en GitHub en su navegador.

**Recursos:** mientras espera detecciones, FrigatePiP prácticamente no usa CPU y ocupa unos 5–10 MB de memoria. Siempre que es posible, el video lo decodifica la tarjeta gráfica. Sin una tarjeta gráfica utilizable (por ejemplo, en algunas máquinas virtuales) dibuja con el renderizador por software WARP.

## 11. Solución de problemas

Empiece con **Probar conexión** en la configuración y con **Mostrar registro** en el menú de la bandeja.

**El icono sigue gris (sin conexión)**
- Compruebe la dirección de Frigate y el puerto en un navegador del mismo equipo: `http://<frigate>:5000/api/version` (o `https://<frigate>:8971/api/version` tras iniciar sesión) debería mostrar la versión.
- En Docker, el puerto (5000 u 8971) debe estar publicado.
- Complemento de Home Assistant: active el puerto 5000 en la configuración de *Red* del complemento o use el puerto 8971 con un usuario de Frigate.
- Puerto 8971 o un proxy inverso: escriba el usuario y la contraseña de Frigate; **Probar conexión** indica si el inicio de sesión funciona. Un certificado autofirmado (el propio de Frigate en el puerto 8971) se acepta la primera vez y se recuerda en `certificates.txt` (junto a `config.ini`); si el servidor muestra después otro, FrigatePiP rechaza la conexión: si cambió el certificado usted mismo, borre su línea en ese archivo.
- Con MQTT: compruebe la dirección del bróker, el inicio de sesión y el prefijo de temas.

**No aparece ninguna ventana**
- ¿El icono está en ámbar? Las notificaciones están en pausa: elija **Reanudar ahora**.
- Revise la pestaña **Detección**: cámaras, objetos y zonas requeridas. Si hay zonas requeridas marcadas, los objetos deben entrar en ellas.
- Los modos *Review* dependen de la configuración de review de Frigate; pruebe **Cada objeto detectado**.
- **No mostrar durante juegos o presentaciones a pantalla completa** omite las ventanas mientras haya una aplicación a pantalla completa en primer plano.
- **Número máx. de ventanas** junto con **Omitir el nuevo evento** omite las detecciones cuando todas las ventanas están abiertas.
- Puede que **Confianza mínima** sea demasiado alta.

**La ventana solo muestra una imagen fija**
- El video en vivo necesita el puerto RTSP 8554 de go2rtc. Revise la línea **Video RTSP** de **Probar conexión**.
- Si go2rtc requiere inicio de sesión, inclúyalo en la **Dirección RTSP**.
- Para cámaras H.265, instale *Extensiones de video HEVC* o use un flujo sub H.264.
- Como solución alternativa, seleccione **MJPEG de Frigate**.
- En Windows N sin el *Media Feature Pack*, el video RTSP no está disponible; FrigatePiP usa MJPEG automáticamente (el registro indica *MJPEG from Frigate instead of RTSP*).

**El video tarda en empezar**
- Active **Búfer previo** y **GPU preparada**.
- Acorte el intervalo de fotogramas clave (intervalo de fotogramas I / GOP) del flujo sub de la cámara a 1–2 s.

**La imagen de una cámara va por detrás de la de otra**
- Las cámaras no comparten un reloj, así que FrigatePiP no puede alinear sus imágenes.
- Cada ventana deja una línea en el registro al cerrarse: *timing - 73 frames, picture in 3 ms (max 96), late up to 177 ms, longest gap 238 ms*. *picture in* es el tiempo dentro de FrigatePiP, *late up to* indica cuánto se retrasaron los fotogramas respecto al ritmo de su flujo y *longest gap* es la pausa más larga entre dos fotogramas.
- Si los valores son pequeños y la imagen sigue retrasada, el retraso se produce antes de que el vídeo llegue a FrigatePiP: compruebe los ajustes del flujo en la cámara. Los valores grandes apuntan a la red o, en *picture in*, al PC.

**No se oye el sonido de la cámara**
- **Audio de la cámara** debe estar marcado e **Imagen en vivo** configurada en **RTSP de go2rtc**.
- El flujo de go2rtc debe contener audio: **Probar conexión** muestra *audio … Hz* o *sin audio*.
- Compruebe el volumen de *FrigatePiP* en el Mezclador de volumen de Windows.

**FrigatePiP se cierra solo**
- Cada línea del registro empieza con la fecha y la hora. Un fallo de FrigatePiP deja una última línea que empieza por *Crash in thread*: adjúntela a un issue (vea más abajo).
- Si el registro simplemente se interrumpe, sin *Shutting down* y sin *Crash*, otra cosa cerró el programa: el apagado o una actualización de Windows, o el cierre del programa que lo inició (por ejemplo, una ventana de terminal).

**Otro problema** — abra un [issue en GitHub](https://github.com/maciejgornicz/frigate-pip/issues) y adjunte la parte relevante del registro. Elimine las direcciones y los nombres de cámaras que no quiera compartir.

---

## 12. Versión para Linux

FrigatePiP también funciona en Linux: en un **escritorio** (GNOME, KDE, Xfce…), como en Windows, y **sin escritorio**, por ejemplo, en una Raspberry Pi con un monitor en la pared o en un servidor al que se accede por SSH. Muestra las mismas ventanas de vista previa, en uno de estos cuatro modos:

| Modo | Dónde aparecen las ventanas | ¿Se ejecuta en segundo plano? | Uso típico | Cómo iniciarlo |
|---|---|---|---|---|
| **Ventanas en el escritorio** | En el escritorio (X11, o Wayland con XWayland): ventanas sin bordes por encima de todas las demás, en la esquina elegida, como en Windows | **Sí** — sin terminal; desde el menú de aplicaciones o con la sesión del escritorio | Un PC o portátil con Linux | Menú → **FrigatePiP** o `frigate-pip` en el escritorio — consulte [Ventanas en el escritorio](#ventanas-en-el-escritorio) |
| **Framebuffer** | En la pantalla de una consola de Linux sin X ni Wayland, en la esquina elegida, sobre el texto de la consola | **Sí** — como servicio de systemd | Una Raspberry Pi u otro equipo con monitor y sin escritorio (una pantalla de pared) | `frigate-pip --output framebuffer`; como servicio: consulte [Inicio automático](#inicio-automático-systemd) |
| **Terminal** | Dentro de la ventana de terminal en la que se ejecuta FrigatePiP: una línea con el texto, la imagen y una línea de estado | **No** — la ventana de terminal muestra FrigatePiP mientras se ejecuta (como `top`) | Una ventana de terminal o un panel de tmux que se deja abierto como monitor de cámaras, también por SSH | `frigate-pip` |
| **Ventanas emergentes de tmux** | Como ventana emergente de tmux sobre lo que esté haciendo en tmux; se cierra sola | **Sí** — en una ventana oculta de tmux | Trabajar en tmux (en local o por SSH) y querer que las cámaras aparezcan en ventanas emergentes | `tmux new -d -s frigate-pip 'frigate-pip --output tmux'` y después trabaje dentro de tmux — consulte [Ventanas emergentes de tmux](#ventanas-emergentes-de-tmux) |

**En un equipo con Windows — también si trabaja en WSL — use la aplicación de Windows:** se ejecuta en el área de notificación y sus ventanas aparecen por encima de todo, incluidas las ventanas de terminal. La versión de Linux en WSL solo es útil para probarla.

Cuando una ventana se cierra en el framebuffer, vuelve a aparecer lo que había debajo. En una ventana de terminal y en las ventanas emergentes de tmux, la imagen se dibuja con **gráficos de Kitty** (Kitty, WezTerm, Ghostty), **Sixel** (foot, Konsole, xterm, Windows Terminal 1.22+) o, en cualquier terminal con color de 24 bits, con medios bloques de colores (baja resolución; las ventanas emergentes de tmux siempre usan medios bloques).

El video lo decodifica el programa **ffmpeg**; el propio cliente RTSP de FrigatePiP le suministra los datos, por lo que la imagen aparece a los pocos milisegundos del siguiente fotograma clave de la cámara.

### Requisitos

- Linux en **x86_64** o **ARM64** (Raspberry Pi 3, 4 o 5 con un sistema de 64 bits).
- Para las ventanas en el escritorio: un escritorio con X11, o Wayland con XWayland (GNOME, KDE Plasma, Xfce y la mayoría de los demás).
- **ffmpeg**: se instala junto con el paquete `.deb`; si no, `sudo apt install ffmpeg` (Debian, Ubuntu, Raspberry Pi OS) o el paquete de su distribución.
- Para el texto en el framebuffer: la fuente DejaVu (`fonts-dejavu-core`, que normalmente se instala junto con ffmpeg).
- Para el framebuffer: pertenecer al grupo `video` (`sudo usermod -aG video $USER` y, después, volver a iniciar sesión).
- Frigate accesible desde el equipo en su puerto 5000 de la API (`http://`) o 8971 (`https://`, con un usuario de Frigate) y, para el video en vivo, el puerto RTSP 8554 de go2rtc (detalles en [Requisitos](#1-requisitos)).

### Instalación

1. Instale el programa:
   - **Debian, Ubuntu, Raspberry Pi OS (64 bits), desde el repositorio de FrigatePiP** (recomendado: así `sudo apt upgrade` también actualiza FrigatePiP): agregue el repositorio una vez e instale el programa; apt también instala ffmpeg:

     ```
     sudo wget -qO /usr/share/keyrings/frigate-pip.gpg https://maciejgornicz.github.io/frigate-pip/apt/frigate-pip.gpg
     echo "deb [signed-by=/usr/share/keyrings/frigate-pip.gpg] https://maciejgornicz.github.io/frigate-pip/apt stable main" | sudo tee /etc/apt/sources.list.d/frigate-pip.list
     sudo apt update
     sudo apt install frigate-pip
     ```

   - **Debian, Ubuntu, Raspberry Pi OS (64 bits), un solo archivo de paquete** — descargue `frigate-pip_<version>_amd64.deb` (o `frigate-pip_<version>_arm64.deb` para una Raspberry Pi) desde la página [Releases](https://github.com/maciejgornicz/frigate-pip/releases) e instálelo con apt, que también instala ffmpeg:

     ```
     sudo apt install ./frigate-pip_*.deb
     ```

   - **Otras distribuciones** — descargue `FrigatePiP-<version>-linux-x86_64.tar.gz` o `FrigatePiP-<version>-linux-arm64.tar.gz`, descomprímalo y coloque el programa en un directorio incluido en el PATH:

     ```
     tar xzf FrigatePiP-*-linux-*.tar.gz
     sudo install frigate-pip /usr/local/bin/
     ```

2. Inícielo una vez: se crea `~/.config/frigate-pip/config.ini`. En la sección `[frigate]`, establezca `url` en la dirección de su servidor Frigate.
3. Compruebe la conexión: `frigate-pip --test-connection`.
4. Inicie `frigate-pip`. Con `--test <camera>` (o las teclas **1–9**) puede ver una cámara de inmediato.

El programa es un único archivo sin otras bibliotecas. Para actualizarlo, ejecute `sudo apt update && sudo apt upgrade` (repositorio), instale el paquete más reciente del mismo modo o reemplace el archivo; para quitarlo, ejecute `sudo apt remove frigate-pip` (o elimine el archivo) y elimine `~/.config/frigate-pip` y, si agregó el repositorio, también `/etc/apt/sources.list.d/frigate-pip.list` y `/usr/share/keyrings/frigate-pip.gpg`.

### Dónde aparecen las ventanas

Con `--output auto` (el valor predeterminado), FrigatePiP abre **ventanas en el escritorio** cuando se ejecuta en un escritorio (`DISPLAY` está definido), usa el modo **framebuffer** en una consola de Linux (`TERM=linux`) o sin terminal (como servicio), y el modo **terminal** en los demás casos. El modo **ventanas emergentes de tmux** solo se usa si usted lo elige (`--output tmux` u `output = tmux`). En una ventana de terminal, el modo de imagen se elige automáticamente: gráficos de Kitty si están disponibles; si no, Sixel, cuando el terminal indica que lo admite; y si no, medios bloques. Ambos se pueden definir en el archivo de configuración o en la línea de comandos.

### Ventanas en el escritorio

En un escritorio Linux, las ventanas de vista previa funcionan como en Windows: ventanas sin bordes por encima de todas las demás, en la esquina elegida (`position`) del monitor elegido (`monitor`: `primary` o un número contado desde la izquierda), con el ancho de `width` (multiplicado por la escala del escritorio). Necesitan X11, o Wayland con XWayland: GNOME, KDE Plasma y la mayoría de los demás escritorios inician XWayland para estos programas. Wayland por sí solo no basta: no permite que un programa coloque una ventana en una esquina ni la mantenga encima.

- **Inicio:** menú de aplicaciones → **FrigatePiP** (instalado con apt) o `frigate-pip` en un terminal del escritorio. Se ejecuta en segundo plano sin terminal; se ejecuta una copia por usuario. Un segundo inicio (o `frigate-pip --settings`) abre la configuración de la copia en ejecución.
- **Con la sesión del escritorio:** `frigate-pip --autostart on` (para desactivarlo: `frigate-pip --autostart off`). Para detenerlo: `frigate-pip --quit`.
- **Icono en la bandeja:** con el mismo menú que en Windows (consulte [El icono de la bandeja](#4-el-icono-de-la-bandeja)); su color muestra el estado: azul — conectado, gris — sin conexión, ámbar — en pausa. Aparece en KDE Plasma, Xfce, Cinnamon, MATE y Budgie, y en GNOME con la extensión AppIndicator (incluida en Ubuntu).
- **Mouse:** como en Windows (consulte [Acciones con el mouse](#acciones-con-el-mouse)): × o el botón derecho cierra, el altavoz silencia, el control bajo él ajusta el volumen, ⟲ repite el inicio del evento, arrastrar mueve la ventana, la rueda cambia su tamaño, Ctrl + rueda acerca la imagen alrededor del puntero, Ctrl + arrastrar mueve la imagen acercada, un clic la fija, un doble clic abre la cámara en Frigate y una ventana bajo el puntero no se cierra.
- **Opciones de las ventanas:** `topmost` (con `false`, las ventanas son ventanas normales que otras pueden tapar), `sound` (el sonido de notificación del escritorio) y `skip_fullscreen` (nada mientras la ventana activa está a pantalla completa) funcionan como en Windows.
- **Configuración:** un clic en el icono (o **Configuración…** en su menú) la abre en el navegador, con las mismas opciones, descripciones y prueba de conexión que en Windows; la página solo se abre en este equipo. En el primer inicio se abre sola. Se guarda en `~/.config/frigate-pip/config.ini` (descrito más abajo).

### Ventanas emergentes de tmux

En este modo, FrigatePiP no tiene ventana propia: espera detecciones en segundo plano y, por cada una, abre una ventana emergente de [tmux](https://github.com/tmux/tmux) con la cámara sobre los paneles del cliente de tmux que usó por última vez. Requiere tmux 3.2 o posterior y FrigatePiP 1.3.0 o posterior (las versiones anteriores no reconocen `--output tmux` y usan el modo terminal sin avisar).

**Las ventanas emergentes solo aparecen mientras trabaja dentro de tmux.** tmux solo puede dibujarlas en su propia pantalla: en una ventana de terminal sin tmux — un shell normal o una sesión SSH fuera de tmux — no se muestra nada y las detecciones solo se escriben en el registro. Por eso no basta con iniciar el proceso en segundo plano; también necesita una sesión de tmux en la que trabajar:

1. Inicie FrigatePiP una vez, en una sesión oculta de tmux: `tmux new -d -s frigate-pip 'frigate-pip --output tmux'`.
2. Abra tmux para su propio trabajo — `tmux` inicia una sesión nueva — y trabaje allí. Las ventanas emergentes aparecen sobre esa ventana.

- Para detener FrigatePiP: `tmux kill-session -t frigate-pip`. Para usar sus teclas (pausa, mostrar una cámara): `tmux attach -t frigate-pip`.
- La ventana emergente aparece en la **esquina** elegida (`position`) y su tamaño sigue el **ancho de la ventana** (`width`). Se cierra sola tras el tiempo habitual (`duration`, `max_duration`) o al instante con **q**, **x** o **Esc**. **m** silencia y reactiva su sonido. **r** repite el inicio del evento (desde que se abrió la ventana emergente).
- tmux muestra una sola ventana emergente a la vez: una detección en otra cámara reemplaza la ventana emergente abierta (o se omite si `replace = false`).
- Para ver la imagen con todos sus colores, tmux debe transmitir el color de 24 bits: agregue `set -as terminal-features ",xterm-256color:RGB"` a `~/.tmux.conf` (use su `$TERM` de fuera de tmux) y reinicie tmux. FrigatePiP escribe esta sugerencia en su registro cuando faltan los colores.

### Teclas

Cuando FrigatePiP se ejecuta en una ventana de terminal o en una consola:

| Tecla | Efecto |
|---|---|
| **q** (o Ctrl+C) | Salir. |
| **x** (o Esc) | Cerrar todas las ventanas de vista previa. |
| **p** | Pausar / reanudar las notificaciones. |
| **m** | Desactiva / activa el sonido (con `camera_audio = true`) en todas las ventanas, también en las que se abran después; el texto muestra entonces *sonido desactivado*. |
| **r** | Repetir el inicio del evento en todas las ventanas (consulte [Acciones con el mouse](#acciones-con-el-mouse)); otra vez: volver al video en vivo. |
| **1**–**9** | Mostrar la cámara 1–9 (en el orden de la configuración de Frigate). |

### Línea de comandos

| Opción | Efecto |
|---|---|
| `--output auto\|window\|framebuffer\|terminal\|tmux` | Dónde se dibujan las ventanas (consulte la tabla de modos más arriba). |
| `--graphics auto\|kitty\|sixel\|blocks` | Cómo se dibujan las imágenes en una ventana de terminal. |
| `--framebuffer /dev/fb0` | Dispositivo framebuffer. |
| `--hwaccel auto\|on\|off` | Decodificación de vídeo en la tarjeta gráfica (vea `hwaccel` más abajo). |
| `--test <camera>` | Mostrar una cámara justo después del inicio. |
| `--test-connection` | Comprobar la conexión con Frigate (como **Probar conexión** en Windows) y salir. |
| `--list` | Muestra las cámaras con sus objetos y zonas — los nombres para `cameras`, `labels` y `zones` en `[trigger]` — y termina. |
| `--autostart on\|off` | Inicia las ventanas en el escritorio con la sesión del escritorio (una entrada en `~/.config/autostart`) o deja de hacerlo. |
| `--quit` | Detiene las ventanas en el escritorio en ejecución. |
| `--settings` | Abre la configuración de las ventanas en el escritorio en ejecución (o las inicia con la configuración abierta). |
| `--version`, `--help` | Versión, ayuda breve. |

### Opciones de configuración

La versión de Linux usa el mismo `config.ini` (en `~/.config/frigate-pip/` o en `$XDG_CONFIG_HOME/frigate-pip/`) que la versión de Windows — consulte [El archivo de configuración](#7-el-archivo-de-configuración). Los cambios se aplican en menos de dos segundos tras guardar el archivo. Se usan estas opciones: todo lo de `[app]` (idioma), `[frigate]`, `[mqtt]`, `[trigger]`, y de `[display]`: `monitor`, `position`, `margin`, `width`, `duration`, `max_duration`, `overlay`, `max_popups`, `replace`, `topmost`, `sound`, `skip_fullscreen`; de `[video]`: `live`, `stream`, `auto_main_width`, `prebuffer` (no para las ventanas emergentes de tmux), `camera_audio`, `wind_filter`; y las secciones `[camera.<name>]`. Las opciones exclusivas de Windows (`gpu_warm`, `check_updates`) se ignoran. `frigate-pip --list` muestra los nombres de sus cámaras, objetos y zonas para los filtros de `[trigger]`.

Opciones adicionales de la sección `[console]`:

| Clave | Predeterminado | Significado |
|---|---|---|
| `output` | `auto` | `auto`, `window`, `framebuffer`, `terminal` o `tmux`. |
| `graphics` | `auto` | `auto`, `kitty`, `sixel` o `blocks` (imágenes en una ventana de terminal). |
| `framebuffer` | `/dev/fb0` | Dispositivo framebuffer. |
| `ffmpeg` | `ffmpeg` | Programa ffmpeg (nombre o ruta completa). |
| `hwaccel` | `auto` | Decodificación de vídeo en la tarjeta gráfica (VAAPI, VDPAU, NVIDIA… lo que encuentre ffmpeg; sin ella, en el procesador): `auto` = para el flujo main, donde más ahorra (la primera imagen de esa ventana puede llegar unos 0,2 s más tarde; el búfer previo lo oculta), `on` = siempre, `off` = nunca. |
| `font` | *(DejaVu Sans Bold)* | Fuente TrueType para el texto en el framebuffer. |

El idioma sigue la configuración regional del sistema (`LANG`), salvo que se defina `language` en `[app]`. El audio de la cámara se reproduce a través de PulseAudio/PipeWire si está disponible; si no, a través de ALSA.

### Inicio automático (systemd)

Para un monitor que muestra la consola (p. ej., una Raspberry Pi sin escritorio), FrigatePiP puede iniciarse como servicio al arrancar el equipo.

**Instalado con apt (repositorio o archivo `.deb`):** el servicio ya viene incluido; basta con activarlo para su usuario (reemplace `pi` por su nombre de usuario): `sudo systemctl enable --now frigate-pip@pi`. Su registro: `journalctl -u frigate-pip@pi`.

**Instalado desde el archivo comprimido:** cree `/etc/systemd/system/frigate-pip.service` (reemplace `pi` por su nombre de usuario):

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

Después: `sudo systemctl enable --now frigate-pip`. El registro se envía al journal (`journalctl -u frigate-pip`) y a `~/.config/frigate-pip/frigate-pip.log`. Para que el cursor parpadeante de la consola no aparezca sobre las ventanas, agregue `vt.global_cursor_default=0` a la línea de comandos del kernel (en una Raspberry Pi: `/boot/firmware/cmdline.txt`).

### Solución de problemas en Linux

- **"cannot start ffmpeg"** — instale ffmpeg (`sudo apt install ffmpeg`) o configure `ffmpeg` en `[console]`.
- **"/dev/fb0: Permission denied"** — agregue el usuario al grupo `video` y vuelva a iniciar sesión (para el servicio: `SupplementaryGroups=video`).
- **No aparece el texto en el framebuffer** — instale `fonts-dejavu-core` o configure `font` en `[console]`.
- **La imagen en la ventana de terminal se ve pixelada** — es el modo de medios bloques; use un emulador de terminal compatible con gráficos de Kitty o Sixel (consulte más arriba) o configure `graphics`.
- **No aparece ninguna ventana emergente de tmux** — las ventanas emergentes solo aparecen mientras trabaja dentro de tmux, no en una ventana de terminal normal (consulte [Ventanas emergentes de tmux](#ventanas-emergentes-de-tmux)). Compruebe que hay un cliente de tmux conectado (`tmux list-clients` lo muestra), que `frigate-pip --version` indica 1.3.0 o posterior, que la versión de tmux es la 3.2 o posterior (`tmux -V`) y que el proceso en segundo plano se está ejecutando (`tmux ls` muestra la sesión `frigate-pip`); el registro indica por qué no se mostró una ventana emergente.
- **Colores pálidos en las ventanas emergentes de tmux** — active el color de 24 bits en tmux (consulte [Ventanas emergentes de tmux](#ventanas-emergentes-de-tmux)).
- **No hay sonido** — el audio de la cámara necesita PulseAudio/PipeWire o ALSA; el usuario del servicio debe pertenecer al grupo `audio`.
- **Para obtener un informe detallado**, inicie FrigatePiP con `FRIGATEPIP_FFMPEG_LOG=/tmp/ffmpeg.log frigate-pip` y adjunte a su issue el final de ese archivo y del registro.
- **No aparecen ventanas en el escritorio** — necesitan X11 o XWayland: `echo $DISPLAY` debe mostrar algo como `:0`. Sin ello, FrigatePiP usa los modos de consola; el registro indica el motivo (`No desktop windows`).
- **FrigatePiP indica que ya se está ejecutando** — se ejecuta una copia por usuario, por ejemplo la iniciada con la sesión del escritorio; `frigate-pip --quit` la detiene.
- **No hay icono en la bandeja en GNOME** — GNOME solo muestra estos iconos con la extensión «AppIndicator and KStatusNotifierItem Support» (paquete `gnome-shell-extension-appindicator`; incluida en Ubuntu). Sin ella, inicie FrigatePiP de nuevo desde el menú de aplicaciones para abrir la configuración.
