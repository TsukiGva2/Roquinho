<p align="center">
  <img src="https://img.shields.io/badge/Rust-1.97-dea584?logo=rust&logoColor=black" alt="Rust 1.97">
  <img src="https://img.shields.io/badge/X11-DOCK-f28834?logo=x.org&logoColor=white" alt="X11">
  <img src="https://img.shields.io/badge/render-tiny--skia_·_CPU-5c5c5c" alt="tiny-skia, renderização por CPU">
  <img src="https://img.shields.io/badge/sem-Qt_·_KDE_·_GUI_toolkit-926ee4" alt="Sem Qt, KDE ou toolkit gráfico">
  <img src="https://img.shields.io/badge/licen%C3%A7a-MIT-green" alt="MIT">
</p>

<h1 align="center">Roquinho 🦀🐱</h1>

<p align="center">
  <b>Roquinho</b> é a reescrita em <b>Rust</b> do <a href="https://github.com/Vitorprogrammerjunior/Foquinho">Foquinho</a>,
  o notch de foco criado pelo <a href="https://github.com/Vitorprogrammerjunior">Vitor</a>.<br>
  Ele fica pendurado no topo da tela e deixa <b>o que você está fazendo agora</b> sempre à vista:
  registra uma demanda em dois segundos, mostra o cronômetro correndo e, quando você termina,
  a próxima da fila já entra sozinha. Tudo com um mascotinho torcendo por você.
</p>

---

## 🔀 Foquinho × Roquinho

**O Foquinho original** (Vitor) é um app em **C++17 / Qt 6 / QML**. Tem uma versão nativa para
**KDE Plasma 6 em Wayland** (layer-shell, atalhos no KGlobalAccel, cor de destaque do KDE) e uma variante para
**Openbox / X11**. Além do notch, ele traz o modo música (MPRIS, DVD girando, ondinhas na batida), a integração com
um Hub de Demandas e o lembrete para levantar.

**O Roquinho** pega a parte do Foquinho que roda em X11 e a reescreve do zero em Rust, sem Qt, sem QML e sem KDE:
uma janela DOCK desenhada direto pela CPU, num único laço `epoll`. A interface foi portada fiel ao original
(layout, animações de mola, mascotes, `state.json` compatível), mas **nem tudo veio junto**: o escopo ficou
só no notch de foco.

|  | Foquinho (original) | Roquinho (esta reescrita) |
|---|---|---|
| Linguagem | C++17 | Rust 2021 (toolchain fixada em 1.97.0) |
| Interface | Qt 6 + QML | Sem toolkit: views escritas à mão |
| Renderização | Qt Quick (GPU/scene graph) | CPU, com [tiny-skia](https://github.com/linebender/tiny-skia); só redesenha os retângulos que mudaram |
| Plataforma | KDE Plasma 6 (Wayland) e Openbox/X11 | **Somente X11** (janela DOCK, MIT-SHM, RandR, xkb) |
| Atalhos globais | KGlobalAccel (KDE) ou `XGrabKey` (Openbox) | `XGrabKey` no X, via `~/.config/foco-maxrc` |
| Cor de destaque | Segue o KDE, ou 9 cores no Openbox | Automática (roxo padrão) ou uma das 8 cores nos ajustes |
| Modo música (MPRIS, DVD, batida) | ✅ | ❌ removido |
| Hub de Demandas | ✅ | ❌ não existe |
| Lembrete para levantar | ✅ | ❌ não existe |
| Mascotes | Gato, fantasma, slime | Gato, fantasma, slime |
| Frases do mascote | Originais | **Novas, com tema punk** ([veja abaixo](#-frases-punk)) |
| CPU parado | Event loop do Qt | Nenhum wake: parado, o processo não acorda (os próximos eventos são prazos exatos) |
| Dependências | Qt 6, layer-shell-qt, kglobalaccel, libpulse | `tiny-skia`, `x11rb`, `xkbcommon`, `rustix`, `serde`, `ttf-parser`, `ab_glyph_rasterizer` + fontconfig (fontes) |
| Formato do `state.json` | `~/.local/share/foco-max/state.json` | O mesmo caminho e o mesmo formato |
| Binário | `foco-max` | `foco-max` (o mesmo nome) |

> Como o formato do `state.json`, o `foco-maxrc` e o nome do binário são os mesmos, as duas versões não devem
> rodar ao mesmo tempo (compartilham o socket de instância única).

---

## ✨ Funcionalidades

- 🎯 **Foco sempre à vista.** O notch no topo da tela mostra a demanda atual e o tempo gasto.
- ⚡ **Registrar é instantâneo.** Um atalho, digita, <kbd>Enter</kbd> manda pra fila, <kbd>Ctrl</kbd>+<kbd>Enter</kbd> põe direto em foco.
- ⏱️ **Cronômetro em cada demanda.** Várias podem ficar em foco ao mesmo tempo, cada uma com seu tempo.
- 🔁 **A fila anda sozinha.** Concluiu? A próxima entra em foco (dá pra desligar nos ajustes).
- 🧲 **Fila que você reorganiza.** Arraste pela alça, dê duplo clique para renomear, use 🎯 para focar e 🗑️ para apagar.
- ✅ **"Destruídas hoje".** As concluídas do dia ficam guardadas, com o tempo gasto, e dá pra devolver uma pra fila.
- 🫧 **Esconda numa bolinha.** Tela limpa com um atalho; clicou na bolinha, tudo volta.
- 🐱 **Mascote com personalidade** que reage a tudo, com nível e barra de XP (um nível a cada 5 demandas concluídas).
- 🖥️ **Escolha o monitor.** Cada tela conectada aparece nos ajustes (nome pelo EDID); se a tela some, o notch se muda sozinho.
- 💬 **Balão de atalhos.** Na abertura e uma vez por dia, com opção de nunca mais mostrar.
- 🚀 **Inicia com o sistema**, pelos ajustes.
- 🪶 **Leve.** Renderização por CPU, só o que mudou é repintado e, sem nada acontecendo, o processo dorme.

---

## 🐾 Conheça o Foquinho

Gato, fantasma ou slime: escolha o seu (e dê um nome a ele). O mascote do Roquinho se chama Foquinho por padrão.

| Quando… | Ele fica… |
|---|---|
| não há nada em foco | 😴 dormindo, com *zzz* |
| você está focado | 😤 de sobrancelha determinada |
| tudo pausado | 😐 em pausa ("intervalo do show") |
| você conclui algo | 🤩 comemorando |
| uma run passa de 50 minutos | 😓 cansado |
| você abre o painel | 👋 acenando |
| você faz carinho nele | 😍 apaixonado |
| ninguém está olhando | 🎵 cantarola, boceja, pisca, mexe a orelha e o rabo |

No painel ele segue o cursor com os olhos.

### 🎸 Frases punk

Os balõezinhos e a linha de status do mascote foram reescritos no Roquinho, com tema punk/rock. Exemplos:

- Dormindo: *"Zzz… no future e nenhuma demanda."* / *"Zzz… 3 na fila. Não vão se fazer sozinhas."*
- Focado: *"Uma por vez. Três acordes bastam."* / *"2 em foco. Isso é caos, não multitarefa."*
- Pausado: *"Pausado. Intervalo do show."*
- Cansado: *"Música do Ramones dura 2 min. Você tá em 50."* (e o status vira *"até punk descansa"*)
- Nova em foco: *"Hey ho, let's go!"*, *"1, 2, 3, 4! Foco!"*, *"Pogo mental iniciado."*
- Anotada na fila: *"Pra fila do gargarejo."*, *"Anotado. Procrastine com estilo."*
- Concluída: *"Mais uma pro setlist!"*, *"Anarchy in the to-do!"*, *"Destruiu. Pogo!"* (o alerta diz *"Destruída!"*)
- Carinho: *"Ronrom… mas sou punk, hein."*, *"Cuidado com o moicano!"*
- Balão de atalhos: *"Ô punk, lembra dos meus atalhos?"*, com os botões *"Valeu, punk!"* e *"Me deixa em paz"*

---

## ⌨️ Atalhos globais

São capturados direto no X (`XGrabKey`). <kbd>Meta</kbd> é a tecla <kbd>Super</kbd>.

| Atalho | Id no `foco-maxrc` | Ação |
|---|---|---|
| <kbd>Meta</kbd>+<kbd>Alt</kbd>+<kbd>N</kbd> | `quick-add` | Nova demanda (abre o campo) |
| <kbd>Meta</kbd>+<kbd>Alt</kbd>+<kbd>F</kbd> | `toggle-panel` | Abrir / fechar o painel |
| <kbd>Meta</kbd>+<kbd>Alt</kbd>+<kbd>H</kbd> | `toggle-hide` | Esconder numa bolinha / mostrar |
| <kbd>Meta</kbd>+<kbd>Alt</kbd>+<kbd>D</kbd> | `complete` | Concluir a demanda em foco |
| <kbd>Meta</kbd>+<kbd>Alt</kbd>+<kbd>M</kbd> | `next-screen` | Mover o notch pro próximo monitor |

Para trocar, edite a seção `[shortcuts]` do `~/.config/foco-maxrc` e reabra o app. Aceita `Meta+Alt+N`,
`Super+Ctrl+F1` ou o jeito do Openbox (`W-A-n`). Valor vazio desliga aquele atalho (útil para usar o `rc.xml` do
Openbox com a [linha de comando](#-linha-de-comando)). Os padrões são gravados no arquivo na primeira execução.

```ini
[shortcuts]
quick-add=Meta+Alt+N
toggle-panel=Meta+Alt+F
complete=
```

---

## 🖥️ Linha de comando

O app é instância única: chamar o binário de novo só manda o comando para o que já está rodando (por um socket
abstrato, sem travar). Sem argumentos, ele alterna o painel.

```bash
foco-max                         # inicia (ou abre/fecha o painel se já estiver rodando)
foco-max start                   # inicia sem abrir nada (usado no autostart)
foco-max add "Revisar PR"        # adiciona à fila (sem texto, abre o campo de digitar)
foco-max focus "Ligar pro João"  # adiciona direto em foco
foco-max done                    # conclui a demanda em foco
foco-max pause                   # pausa / retoma tudo
foco-max open | toggle           # abre o painel / alterna
foco-max hide | show | quit
foco-max screen next             # próximo monitor
foco-max screen HDMI-A-1         # uma tela específica (ou: screen primary)
```

Para ver a interface sem servidor X, há um modo de captura que gera um PNG a partir de um `state.json`:

```bash
foco-max --snapshot state.json saida.png [collapsed|expanded|settings|alert|tip|dot]
```

---

## ⚙️ Ajustes

- Escolha o mascote (gato, fantasma ou slime) e dê um nome a ele
- Ligue ou desligue a fila automática
- Inicie junto com o sistema (`~/.config/autostart` e, se existir, o `~/.config/openbox/autostart`)
- Ative ou não o balão de dicas de atalhos
- Escolha em qual monitor ele fica (ou **Principal**)
- Escolha a cor de destaque: automática ou uma das 8 cores

O estado fica em `~/.local/share/foco-max/state.json` e as preferências de tela, cor e atalhos em
`~/.config/foco-maxrc`.

---

## 📦 Instalação

Só X11. No Arch Linux (o que o CI usa), você precisa de:

```bash
sudo pacman -S --needed base-devel git rustup pkgconf fontconfig libxkbcommon xkeyboard-config libx11 noto-fonts inter-font ttf-dejavu
rustup toolchain install     # lê o rust-toolchain.toml (1.97.0)
```

Clone, compile e instale:

```bash
git clone https://github.com/TsukiGva2/Roquinho.git
cd Roquinho
make install      # compila em release e instala em ~/.local/bin/foco-max
```

Outros alvos: `make deploy` (compila, instala e reinicia a instância que está rodando), `make restart`,
`make stop` e `make uninstall`.

> ⚠️ O `.cargo/config.toml` compila com `target-cpu=native` e o `make build` usa LTO total (é lento). O binário
> resultante **não é portável** para outras CPUs: compile na máquina em que vai rodar.

> 💡 As fontes são resolvidas pelo fontconfig. Para usar a Inter, rode com `FOCO_FONT=inter`.

---

## 🧠 Arquitetura

Workspace Rust com sete crates. As dependências só apontam para `foco-base`; irmãos nunca se ligam entre si,
quem junta tudo é o `foco-max`.

```
foco-base            (base: geometria, cor, animação, eventos, ações, modelo, trait Surface, tempo, wake)
 ^    ^    ^    ^
 |    |    |    foco-pet   mascote (definições, expressões, desenho)
 |    |    foco-core        máquina de estados da store, IPC/CLI, reator epoll, rc, autostart
 |    foco-gfx              renderizador CPU: Canvas (tiny-skia), fontes, glifos, texto, ícones, sombras
 foco-x11                   janela DOCK, MIT-SHM, RandR, xkb, clipboard, key grabs
foco-ui  -> base, core, pet   widgets e views (sem gfx, sem x11)
foco-max -> tudo              app.rs (cena), main.rs (inicialização e laço de frames), trace.rs
```

- A store é pura: `Store::apply(Action, now) -> Vec<Effect>`. Os efeitos colaterais são executados pelo app.
- Cada frame é `update → layout → paint → next_wake`. Os wakes são prazos exatos; ocioso, o processo nunca acorda.
- Detalhes (modelo de dano, wakes, entrada, armadilhas de paridade com o original): [`docs/ui-design.md`](docs/ui-design.md).

Os cronômetros são honestos: se o computador desligar com uma demanda rodando, o tempo conta só até a última vez
em que o app esteve vivo.

---

## 🛠️ Desenvolvimento

```bash
make check    # fmt --check + clippy -D warnings + testes (é o que o CI roda)
make fmt      # cargo fmt --all
make test     # cargo test --workspace
cargo test -p foco-ui views::queue   # um crate / um teste
```

Os testes **não precisam de servidor X**. Eles precisam das fontes do Arch e das tabelas de Compose do libx11
(o teste de teclado usa as teclas mortas do pt_BR). O CI (`.github/workflows/ci.yml`) roda num container Arch.
Os testes de desempenho são `#[ignore]` e só em release:
`cargo test --release -p foco-pet -- --ignored --nocapture` (idem com `-p foco-gfx`).
Para depurar, `FOCO_TRACE=1` imprime um trace dos eventos no stderr.

Em máquinas com pouca RAM, use `CARGO_BUILD_JOBS=2`. As regras do repositório estão no [`CLAUDE.md`](CLAUDE.md).

---

## 🙏 Créditos

O Foquinho (ideia, design, mascotes, interface e textos originais) é do
[Vitor](https://github.com/Vitorprogrammerjunior): <https://github.com/Vitorprogrammerjunior/Foquinho>.
O Roquinho é uma reescrita independente em Rust, que segue o comportamento do original para o notch de foco.

<p align="center">
  Feito com 💜 e 🦀 para quem tem mil abas abertas e um foco só.
</p>
