# FrigatePiP — Guia do utilizador

**Languages:** [English](../en/user-guide.md) · [Polski](../pl/user-guide.md) · [Deutsch](../de/user-guide.md) · [Español](../es/user-guide.md) · [Português](../pt/user-guide.md) · [Français](../fr/user-guide.md)

O FrigatePiP é uma pequena aplicação da área de notificação do Windows para o [Frigate NVR](https://frigate.video). Quando o Frigate deteta uma pessoa, um carro ou qualquer outro objeto, o FrigatePiP mostra de imediato, num canto do ecrã, uma pequena janela picture-in-picture com o vídeo ao vivo dessa câmara. Quando deixa de haver movimento, a janela fecha-se sozinha.

## Índice

1. [Requisitos](#1-requisitos)
2. [Instalação](#2-instalação)
3. [Primeira execução](#3-primeira-execução)
4. [O ícone na área de notificação](#4-o-ícone-na-área-de-notificação)
5. [Janelas de pré-visualização](#5-janelas-de-pré-visualização)
6. [A janela de configurações](#6-a-janela-de-configurações)
7. [O ficheiro de configurações](#7-o-ficheiro-de-configurações)
8. [Linha de comandos](#8-linha-de-comandos)
9. [Idioma](#9-idioma)
10. [Ficheiros, privacidade e recursos](#10-ficheiros-privacidade-e-recursos)
11. [Resolução de problemas](#11-resolução-de-problemas)

---

## 1. Requisitos

- **Windows 10 (versão 1809 ou posterior) ou Windows 11**, 64 bits.
- **Frigate** acessível a partir do PC através da rede. Recomenda-se o Frigate 0.14 ou posterior; os modos *Review* exigem a versão 0.14 ou posterior.
- A **porta 5000 da API** do Frigate tem de estar acessível a partir do PC. O FrigatePiP não inicia sessão, por isso não é possível usar a porta autenticada 8971. Com Docker, publique a porta, p. ex. `- "5000:5000"`.
- **Add-on do Frigate para o Home Assistant:** a porta 5000 está desativada por predefinição — ative-a nas definições de *Rede* do add-on. Endereços HTTPS não são suportados: use o endereço `http://` normal da rede local.
- Para vídeo ao vivo fluido: o **servidor RTSP go2rtc, porta 8554**, integrado no Frigate (`- "8554:8554"`). Sem ele, pode continuar a usar MJPEG ou instantâneos (ver [Imagem ao vivo](#imagem-ao-vivo)).
- Opcional: um **broker MQTT** usado pelo Frigate, se preferir o MQTT ao WebSocket do Frigate.
- Para câmaras H.265 (HEVC), o Windows precisa de um descodificador HEVC. A maioria dos PCs com um driver gráfico recente tem um; caso contrário, instale as *Extensões de Vídeo HEVC* a partir da Microsoft Store ou use um stream sub em H.264. Se não houver descodificador disponível, o FrigatePiP mostra automaticamente MJPEG do Frigate.
- **As edições N do Windows** (por exemplo, Windows 11 Pro N) não incluem o Media Foundation até adicionar a funcionalidade opcional do Windows *Media Feature Pack*. O FrigatePiP funciona na mesma sem ela, mas a imagem ao vivo chega como MJPEG do Frigate (cerca de 5 imagens por segundo) e o áudio AAC da câmara não está disponível.

## 2. Instalação

### Com o instalador (recomendado)

1. Transfira `FrigatePiP-Setup-<version>.exe` da página [Releases](https://github.com/maciejgornicz/frigate-pip/releases).
2. Execute-o. O instalador não precisa de direitos de administrador; instala apenas para o utilizador atual, em `%LOCALAPPDATA%\Programs\FrigatePiP`.
3. Escolha se o FrigatePiP deve **iniciar com o Windows** (ativado por predefinição) e se deve ser criado um atalho no ambiente de trabalho.
4. No final, deixe **Executar FrigatePiP** marcado. O ícone aparece na área de notificação e a janela de configurações abre-se na primeira execução.

O instalador e a aplicação não estão assinados com um certificado de assinatura de código. O Windows SmartScreen pode mostrar *«O Windows protegeu o seu PC»*; clique em **Mais informações → Executar mesmo assim**. Pode verificar a soma de verificação SHA-256 publicada com cada versão.

### Versão portátil (sem instalação)

Transfira `frigate-pip.exe` da página Releases e execute-o a partir de qualquer pasta. As configurações são guardadas no mesmo local que com o instalador (ver [Ficheiros](#10-ficheiros-privacidade-e-recursos)). Use **Iniciar o FrigatePiP com o Windows** nas configurações para o iniciar automaticamente.

### Atualização

O FrigatePiP procura uma versão nova uma vez por dia (**Procurar atualizações** nas configurações). Quando há uma disponível, aparece uma notificação e tanto o menu da área de notificação como a janela de configurações oferecem **Atualizar para …**: o FrigatePiP transfere o instalador do GitHub, verifica a sua soma de verificação SHA-256 e executa-o — a atualização demora alguns segundos e mantém as suas configurações. **Verificar agora** nas configurações procura de imediato uma versão nova. Também pode executar o instalador de uma versão nova por si. A versão portátil apenas abre a página da versão: transfira o novo exe e substitua o antigo.

### Desinstalação

**Definições → Aplicações → Aplicações instaladas → FrigatePiP → Desinstalar** (ou *Adicionar ou remover programas*). O desinstalador fecha o FrigatePiP e remove o programa, os atalhos e a entrada de início automático. As suas configurações e o registo em `%APPDATA%\FrigatePiP` são mantidos; elimine essa pasta para os remover também.

## 3. Primeira execução

1. Na primeira execução, a janela de configurações abre-se no separador **Conexão**.
2. Introduza o **Endereço do Frigate**, p. ex. `http://192.168.1.10:5000`.
3. Clique em **Testar conexão**. O FrigatePiP verifica, sem guardar nada:
   - a API do Frigate e a sua versão,
   - a lista de câmaras,
   - a origem dos eventos (WebSocket ou MQTT),
   - o vídeo RTSP do go2rtc, incluindo o codec e se a câmara tem áudio.

   Cada linha começa com ✔ (funciona) ou ✖ (problema, com uma sugestão).
4. Clique em **OK**. A partir de agora, o FrigatePiP aguarda deteções em segundo plano.
5. Para ver o aspeto de uma janela, clique com o botão direito no ícone da área de notificação e escolha **Mostrar câmara → (uma câmara)**, ou marque **Mostrar ao vivo** na parte inferior da janela de configurações.

## 4. O ícone na área de notificação

O ícone na área de notificação mostra o estado:

| Cor | Significado |
|---|---|
| Azul | Conectado ao Frigate, à espera de deteções |
| Cinzento | Sem conexão (o FrigatePiP continua a tentar automaticamente) |
| Âmbar | As notificações estão em pausa |

Passe o rato por cima do ícone para ver o estado (por exemplo *Conectado: http://192.168.1.10:5000*).

- **Clique esquerdo** abre a janela de configurações.
- **Clique direito** abre o menu:

| Item do menu | O que faz |
|---|---|
| *(linha de estado)* | Estado da conexão ou tempo de pausa restante (informativo). |
| **Atualizar para …** | Aparece quando há uma versão nova disponível: transfere-a e instala-a (consulte [Atualização](#atualização)). |
| **Configurações…** | Abre a janela de configurações. |
| **Mostrar câmara** | Abre de imediato uma janela de pré-visualização da câmara escolhida, mesmo com as notificações em pausa. Útil para verificar a imagem ou simplesmente para ver. |
| **Pausar notificações** | **Durante 15 minutos**, **Durante 1 hora** ou **Até retomar**. Durante a pausa, as deteções não abrem janelas. Enquanto uma pausa está ativa, o item mostra **Em pausa**, e **Retomar agora** termina-a. |
| **Fechar janelas de pré-visualização** | Fecha todas as janelas de pré-visualização abertas, incluindo as fixadas. |
| **Abrir o Frigate** | Abre a interface web do Frigate no browser. |
| **Ajuda** | Abre este guia. |
| **Mostrar registo** | Abre o ficheiro de registo no Bloco de Notas (útil quando algo não funciona). |
| **Ficheiro de configurações (avançado)…** | Abre o `config.ini` no Bloco de Notas. As alterações guardadas aplicam-se de imediato. |
| **Sair** | Fecha o FrigatePiP. |

## 5. Janelas de pré-visualização

Quando uma deteção corresponde às suas configurações, aparece uma janela de pré-visualização no canto do ecrã escolhido:

1. A janela aparece de imediato, poucos milissegundos após a mensagem do Frigate.
2. Primeiro mostra o último instantâneo da câmara feito pelo Frigate e depois passa para o vídeo ao vivo assim que o stream fornece uma imagem (normalmente em menos de um segundo, quase instantaneamente com o [pré-buffer](#pré-buffer)).
3. A legenda (se ativada) mostra o nome da câmara, o objeto (por exemplo *pessoa*), a subetiqueta (por exemplo um rosto ou uma placa de matrícula reconhecidos), a confiança e a hora. Um **ponto vermelho** significa que a imagem é ao vivo.

As janelas de pré-visualização nunca recebem o foco do teclado, por isso não interrompem a escrita nem os jogos.

### Ações do rato

| Ação | Efeito |
|---|---|
| **Arrastar** | Move a janela. Fica onde a colocar até se fechar. |
| **Clique** | **Fixa** a janela: fica aberta até a fechar (a legenda mostra *fixada*). Clique novamente para a desafixar. |
| **Roda do rato** | Aumenta ou diminui a janela. |
| **×** (canto superior direito) | Fecha a janela de imediato. O botão aparece quando aponta o rato para a janela. |
| **Clique direito** | Fecha a janela. |
| **Duplo clique** | Abre esta câmara na interface web do Frigate. |
| **Passar o rato por cima** | Enquanto o ponteiro do rato estiver sobre a janela, esta não se fecha. Depois de afastar o rato, fica aberta pelo menos mais 3 segundos. |

### Quando é que uma janela se fecha?

- Passados os segundos definidos em **Tempo após o último movimento** desde o último movimento do objeto (ou de qualquer outro objeto correspondente nessa câmara).
- O mais tardar após os segundos definidos em **no máximo**, mesmo que o objeto continue a mover-se.
- Nunca enquanto estiver fixada ou enquanto o ponteiro do rato estiver sobre ela.

### Mais do que uma câmara

Cada câmara tem no máximo uma janela. Novas deteções numa câmara que já tem uma janela apenas a prolongam e atualizam a legenda. As janelas de câmaras diferentes empilham-se a partir do canto escolhido. **Número máx. de janelas** limita quantas podem estar abertas ao mesmo tempo; **Quando todas estão em uso** decide se uma nova câmara substitui a janela mais antiga (no mesmo lugar do ecrã; as janelas fixadas nunca são substituídas) ou é ignorada.

## 6. A janela de configurações

Abra-a com um clique esquerdo no ícone da área de notificação ou com **Configurações…** no menu. **Cada opção tem uma descrição: passe o rato por cima dela.** Prima **F1** ou clique em **Ajuda** para abrir este guia.

A maioria das alterações tem efeito **de imediato** nas janelas de pré-visualização abertas, por isso pode ver o resultado antes de guardar. **OK** guarda e fecha, **Aplicar** guarda e mantém a janela aberta, **Cancelar** (ou fechar a janela) descarta as alterações não guardadas.

### 6.1 Conexão

| Opção | Descrição |
|---|---|
| **Endereço do Frigate** | Endereço do servidor Frigate com a porta, p. ex. `http://192.168.1.10:5000` ou `http://frigate.local:5000`. A porta 5000 não requer autenticação. |
| **Origem dos eventos** | **WebSocket do Frigate (recomendado)** — o FrigatePiP conecta-se diretamente ao Frigate; não é necessário mais nada. **MQTT** — as deteções são lidas a partir do seu broker MQTT (o configurado no Frigate). Ambas dão o mesmo resultado; o WebSocket é normalmente um pouco mais rápido. |
| **Servidor MQTT**, **porta** | Nome do host ou endereço IP e porta (normalmente 1883) do broker MQTT. Usado apenas com a origem MQTT. |
| **Utilizador / senha** | Credenciais MQTT. Deixe em branco se o broker permitir acesso anónimo. |
| **Prefixo de tópicos MQTT** | `mqtt.topic_prefix` da configuração do Frigate, normalmente `frigate`. |
| **Endereço RTSP (go2rtc)** | Modelo do endereço do vídeo ao vivo. `{host}` é substituído pelo host do endereço do Frigate e `{stream}` pelo nome do stream go2rtc. O valor predefinido `rtsp://{host}:8554/{stream}` serve para uma instalação padrão do Frigate. Se o go2rtc exigir autenticação, use `rtsp://user:password@{host}:8554/{stream}`. |
| **Testar conexão** | Verifica os valores indicados acima (Frigate, câmaras, eventos, vídeo e áudio RTSP) sem os guardar. |
| **Idioma** | Idioma do FrigatePiP. **Automático (Windows)** segue o idioma de apresentação do Windows; se esse idioma não estiver disponível, é usado o inglês. Ver [Idioma](#9-idioma). |
| **Iniciar o FrigatePiP com o Windows** | Inicia o FrigatePiP automaticamente quando inicia sessão no Windows. |
| **Procurar atualizações** | Uma vez por dia, o FrigatePiP pergunta ao GitHub se existe uma versão nova e, se existir, mostra uma notificação e propõe a atualização. Não é enviado nenhum dado sobre si ou as suas câmaras. |
| **Verificar agora** / **Atualizar para …** | Procura de imediato uma versão nova; se a encontrar, o botão transfere-a e instala-a (as configurações são mantidas). O texto ao lado mostra o resultado. |

### 6.2 Deteção

| Opção | Descrição |
|---|---|
| **Mostrar janela para** | Que mensagens do Frigate abrem uma janela: **Cada objeto detetado (mais rápido)** — cada objeto rastreado pelo Frigate; **Apenas alertas do Review** — apenas itens do Review com gravidade *alert*; **Alertas e deteções do Review** — todos os itens do Review. Os itens do Review seguem as regras de alertas e deteções (etiquetas e zonas obrigatórias) definidas na configuração `review` do Frigate e chegam um pouco mais tarde do que os eventos de objetos. |
| **Câmaras** | Câmaras que podem abrir uma janela. As câmaras desmarcadas são ignoradas. Com todas as câmaras marcadas, as câmaras adicionadas mais tarde ao Frigate também são incluídas. |
| **Objetos** | Tipos de objeto (etiquetas) que podem abrir uma janela, p. ex. pessoa, carro, cão. A lista mostra os objetos rastreados pelas suas câmaras. Com todos marcados, conta qualquer tipo de objeto. |
| **Zonas obrigatórias** | Se marcar zonas, a janela só abre quando o objeto entra numa delas. Sem nenhuma marcada, conta qualquer ponto da imagem. |
| **Ignorar objetos parados** | Os objetos que o Frigate marca como parados (por exemplo um carro estacionado) não abrem janelas nem as mantêm abertas. Aplica-se a *Cada objeto detetado*. |
| **Confiança mínima** | As deteções com confiança inferior são ignoradas. 0% significa que decidem os limiares do próprio Frigate. Aplica-se a *Cada objeto detetado*. |

Os falsos positivos do Frigate são sempre ignorados. Cada objeto abre uma janela apenas uma vez; quando volta a mover-se, apenas prolonga a janela aberta.

### 6.3 Janela de pré-visualização

| Opção | Descrição |
|---|---|
| **Monitor** | Monitor em que as janelas aparecem: **Principal** ou um monitor específico (numerados a partir da esquerda, com a sua resolução). |
| **Canto do ecrã** | Canto em que as janelas aparecem. As janelas seguintes empilham-se a partir deste canto. |
| **Largura da janela** | Largura em pixels (com escala a 100%; o FrigatePiP ajusta-a à escala do monitor). A altura segue a proporção da câmara. Também pode alterar o tamanho de uma única janela com a roda do rato. |
| **Distância à margem** | Espaço entre as janelas e as margens do ecrã (e entre as janelas). |
| **Tempo após o último movimento** | Quantos segundos a janela fica aberta após o último movimento. |
| **no máximo** | Tempo máximo, em segundos, durante o qual uma janela é mostrada, mesmo que o objeto continue a mover-se (a não ser que esteja fixada). |
| **Número máx. de janelas** | Quantas janelas (câmaras) podem estar abertas ao mesmo tempo (1–8). |
| **Quando todas estão em uso** | **Substituir a janela mais antiga (no mesmo lugar)** — a nova câmara ocupa o lugar da janela não fixada mais antiga; **Ignorar o novo evento** — a nova deteção é ignorada. |
| **Sempre em primeiro plano** | Mantém as janelas acima de todas as outras janelas. |
| **Legenda na imagem** | Mostra a câmara, o objeto, a confiança e a hora na imagem. |
| **Som de notificação** | Reproduz o som *Asterisco* do Windows quando aparece uma nova janela. Não é o áudio da câmara (ver [Áudio da câmara](#áudio-da-câmara)). |
| **Não mostrar durante jogos ou apresentações em ecrã inteiro** | Não aparecem janelas enquanto estiver em execução um jogo, vídeo ou apresentação em ecrã inteiro (as deteções são ignoradas, não adiadas). |

### 6.4 Vídeo e áudio

#### Imagem ao vivo

| Opção | Descrição |
|---|---|
| **RTSP do go2rtc** (predefinição) | Vídeo fluido à taxa de quadros da câmara com descodificação por hardware (H.264 e H.265), baixa latência. Requer a porta 8554. |
| **MJPEG do Frigate** | Cerca de 5 quadros por segundo na resolução de deteção, através da porta 5000. Use-o quando o RTSP não estiver disponível. |
| **Apenas instantâneos atualizados** | Apenas os instantâneos do Frigate, atualizados regularmente. A opção mais leve. |

#### Stream

| Opção | Descrição |
|---|---|
| **Automático (pelo tamanho da janela)** | O stream **sub** leve para janelas normais e o stream **main** em resolução total quando a janela for mais larga do que **Main a partir da largura** (em pixels físicos). |
| **Sub (leve)** | Sempre o stream sub. |
| **Main (resolução total)** | Sempre o stream main. |

O FrigatePiP encontra os streams go2rtc de cada câmara na configuração do Frigate (`live.streams`, `live.stream_name` ou streams go2rtc com os nomes `<camera>` e `<camera>_sub`). Pode defini-los manualmente no [ficheiro de configurações](#configurações-por-câmara).

#### Áudio da câmara

**Áudio da câmara** reproduz o som da câmara (AAC ou G.711 do stream RTSP) em cada janela aberta. Funciona em separado, por isso nunca atrasa a janela nem o vídeo. O volume pode ser ajustado no Misturador de volume do Windows (*FrigatePiP*). Requer **RTSP do go2rtc**.

#### Pré-buffer

**Pré-buffer** mantém uma conexão ao stream sub de cada câmara, para que o vídeo ao vivo comece de imediato (cerca de 0,15 s) em vez de esperar pelo próximo quadro-chave (até cerca de 1 s, dependendo do intervalo de quadros-chave da câmara). Custo: cerca de 0,1–0,5 Mbit/s de tráfego na rede local por câmara, permanentemente.

#### GPU pronta

**GPU pronta** mantém o dispositivo gráfico inicializado, para que o vídeo comece cerca de 130 ms mais cedo. Custo: cerca de 1% de um núcleo de CPU em repouso (thread do driver gráfico).

### 6.5 Parte inferior da janela

| Controlo | Descrição |
|---|---|
| **Pré-visualização:** câmara | Câmara usada na pré-visualização ao vivo das configurações. |
| **Mostrar ao vivo** | Abre uma janela de pré-visualização real dessa câmara. Esta reflete as suas alterações (canto, tamanho, legenda, stream, áudio…) de imediato, antes de as guardar. Fica fixada e não conta para o limite de janelas. |
| **OK** / **Cancelar** / **Aplicar** | Guardar e fechar / descartar / guardar e manter a janela aberta. |
| **Ajuda** | Abre este guia (também **F1**). |

## 7. O ficheiro de configurações

Todas as configurações são guardadas em `%APPDATA%\FrigatePiP\config.ini` (um ficheiro de texto simples). A janela de configurações escreve todas elas; o ficheiro só é necessário para as configurações por câmara. **Ficheiro de configurações (avançado)…** no menu da área de notificação abre-o no Bloco de Notas. As alterações são aplicadas automaticamente quando o ficheiro é guardado. Os comentários começam com `;`. Os valores sim/não aceitam `true`/`false`, `yes`/`no`, `on`/`off`, `1`/`0`.

| Secção | Chave | Predefinição | Significado |
|---|---|---|---|
| `[app]` | `language` | `auto` | `auto` (idioma do Windows) ou `en`, `pl`, `de`, `es`, `pt`, `fr`. |
| | `check_updates` | `true` | Procurar uma versão nova uma vez por dia. |
| `[frigate]` | `url` | `http://frigate.local:5000` | Endereço do Frigate. |
| | `source` | `websocket` | `websocket` ou `mqtt`. |
| | `rtsp` | `rtsp://{host}:8554/{stream}` | Modelo do endereço RTSP. |
| `[mqtt]` | `host` | *(vazio)* | Broker MQTT (apenas com `source = mqtt`). |
| | `port` | `1883` | Porta do broker. |
| | `user`, `password` | *(vazio)* | Credenciais. |
| | `prefix` | `frigate` | Prefixo de tópicos. |
| `[trigger]` | `mode` | `events` | `events` (cada objeto), `alerts` (alertas do Review) ou `reviews` (alertas e deteções). |
| | `cameras` | *(vazio = todas)* | Nomes de câmaras separados por vírgulas, p. ex. `front_door, garden`. |
| | `labels` | *(vazio = todos)* | Objetos separados por vírgulas, p. ex. `person, car`. |
| | `zones` | *(vazio = qualquer lugar)* | Zonas obrigatórias. |
| | `ignore_stationary` | `true` | Ignorar objetos parados. |
| | `min_score` | `0` | Confiança mínima 0.0–1.0. |
| `[display]` | `monitor` | `primary` | `primary` ou o número do monitor a partir da esquerda (1, 2, 3…). |
| | `position` | `bottom-right` | `top-left`, `top-right`, `bottom-left`, `bottom-right`. |
| | `margin` | `16` | Distância à margem do ecrã em pixels. |
| | `width` | `480` | Largura da janela em pixels. |
| | `duration` | `15` | Segundos após o último movimento. |
| | `max_duration` | `120` | Máximo de segundos durante os quais uma janela é mostrada. |
| | `topmost` | `true` | Sempre em primeiro plano. |
| | `sound` | `false` | Som de notificação. |
| | `overlay` | `true` | Legenda na imagem. |
| | `max_popups` | `4` | Número máx. de janelas. |
| | `replace` | `true` | `true` = substituir a janela mais antiga, `false` = ignorar o novo evento. |
| | `skip_fullscreen` | `false` | Não mostrar durante aplicações em ecrã inteiro. |
| `[video]` | `live` | `rtsp` | `rtsp`, `mjpeg` ou `snapshot`. |
| | `stream` | `auto` | `auto`, `sub` ou `main`. |
| | `auto_main_width` | `800` | Largura da janela (px) a partir da qual `auto` usa o stream main. |
| | `prebuffer` | `false` | Pré-buffer. |
| | `gpu_warm` | `false` | GPU pronta. |
| | `camera_audio` | `false` | Áudio da câmara. |

### Configurações por câmara

Adicione uma secção `[camera.<name>]`, em que `<name>` é o nome da câmara no Frigate:

```ini
[camera.front_door]
; Name shown in the caption, the menu and the settings
name = Front door
; go2rtc stream names
main = front_door
sub = front_door_sub
```

## 8. Linha de comandos

| Comando | Efeito |
|---|---|
| `frigate-pip.exe` | Inicia o FrigatePiP (só é executada uma cópia de cada vez). |
| `frigate-pip.exe --settings` | Abre a janela de configurações (inicia o FrigatePiP se não estiver em execução). |
| `frigate-pip.exe --test <camera>` | Mostra de imediato uma janela de pré-visualização da câmara, p. ex. `--test front_door`. Pode criar um atalho do Windows com uma tecla de atalho para a sua câmara favorita. |
| `frigate-pip.exe --quit` | Fecha o FrigatePiP em execução. |

Se o FrigatePiP já estiver em execução, o comando é transmitido ao programa em execução.

## 9. Idioma

O FrigatePiP está disponível em inglês, polaco, alemão, espanhol, português e francês. Com **Automático (Windows)**, usa o idioma de apresentação do Windows (*Definições → Hora e idioma → Idioma e região → Idioma de apresentação do Windows*) ou o inglês, se esse idioma não estiver disponível. Pode escolher um idioma manualmente em **Conexão → Idioma**; a janela de configurações muda de imediato.

## 10. Ficheiros, privacidade e recursos

| O quê | Onde |
|---|---|
| Programa | `%LOCALAPPDATA%\Programs\FrigatePiP\frigate-pip.exe` (instalador) |
| Configurações | `%APPDATA%\FrigatePiP\config.ini` |
| Registo | `%APPDATA%\FrigatePiP\frigate-pip.log` (até 512 KB; o anterior é mantido como `frigate-pip.old.log`) |
| Início automático | Valor `FrigatePiP` do Registo do Windows em `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` |

**Privacidade:** o FrigatePiP conecta-se apenas aos endereços que introduzir (Frigate, go2rtc e o broker MQTT) e, a menos que desative **Procurar atualizações**, uma vez por dia ao GitHub para procurar uma versão nova. Não tem telemetria, contas nem serviços na nuvem. As imagens são apenas apresentadas e nunca são guardadas. Os links **Ajuda** abrem este guia no GitHub, no browser.

**Recursos:** enquanto aguarda deteções, o FrigatePiP praticamente não usa CPU e usa cerca de 5–10 MB de memória. O vídeo é descodificado pela placa gráfica sempre que possível. Sem uma placa gráfica utilizável (por exemplo, em algumas máquinas virtuais), desenha com o renderizador por software WARP.

## 11. Resolução de problemas

Comece por **Testar conexão** nas configurações e por **Mostrar registo** no menu da área de notificação.

**O ícone continua cinzento (sem conexão)**
- Verifique o endereço do Frigate e a porta (5000) num browser no mesmo PC: `http://<frigate>:5000/api/version` deve mostrar a versão.
- No Docker, a porta 5000 tem de estar publicada. A porta 8971 (com autenticação) não é suportada.
- Add-on do Home Assistant: ative a porta 5000 nas definições de *Rede* do add-on.
- O endereço tem de começar por `http://`. HTTPS (por exemplo, através de um reverse proxy) não é suportado.
- Com MQTT: verifique o endereço do broker, as credenciais e o prefixo de tópicos.

**Não aparece nenhuma janela**
- O ícone está âmbar? As notificações estão em pausa — escolha **Retomar agora**.
- Verifique o separador **Deteção**: câmaras, objetos e zonas obrigatórias. Com zonas obrigatórias marcadas, os objetos têm de entrar nelas.
- Os modos *Review* dependem da configuração de review do Frigate; experimente **Cada objeto detetado**.
- **Não mostrar durante jogos ou apresentações em ecrã inteiro** ignora as janelas enquanto uma aplicação em ecrã inteiro estiver em primeiro plano.
- **Número máx. de janelas** com **Ignorar o novo evento** ignora as deteções quando todas as janelas estão abertas.
- **Confiança mínima** pode estar demasiado alta.

**A janela mostra apenas uma imagem estática**
- O vídeo ao vivo requer a porta RTSP 8554 do go2rtc. Verifique a linha **Vídeo RTSP** de **Testar conexão**.
- Se o go2rtc exigir autenticação, inclua as credenciais no **Endereço RTSP**.
- Para câmaras H.265, instale as *Extensões de Vídeo HEVC* ou use um stream sub em H.264.
- Como alternativa, selecione **MJPEG do Frigate**.
- No Windows N sem o *Media Feature Pack*, o vídeo RTSP não está disponível; o FrigatePiP usa MJPEG automaticamente (o registo indica *MJPEG from Frigate instead of RTSP*).

**O vídeo demora a começar**
- Ative **Pré-buffer** e **GPU pronta**.
- Reduza o intervalo de quadros-chave (intervalo de I-frames / GOP) do stream sub da câmara para 1–2 s.

**Sem som da câmara**
- **Áudio da câmara** tem de estar marcado e **Imagem ao vivo** definida como **RTSP do go2rtc**.
- O stream go2rtc tem de conter áudio — **Testar conexão** mostra *áudio … Hz* ou *sem áudio*.
- Verifique o volume de *FrigatePiP* no Misturador de volume do Windows.

**Outro problema** — abra uma [issue no GitHub](https://github.com/maciejgornicz/frigate-pip/issues) e anexe a parte relevante do registo. Remova os endereços e os nomes de câmaras que não pretende partilhar.
