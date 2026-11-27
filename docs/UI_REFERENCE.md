# Referências visuais do qpdfview

**Referências locais:** `/home/elton/Downloads/qpdfview/img` | **Inventário revisto em:** 2026-11-26

Este documento registra o que as capturas permitem comparar na interface do gpdfview. As imagens permanecem na pasta local de referência e não são copiadas nem redistribuídas pelo projeto.

## Estados cobertos

| Arquivos | Estado observado | O que comparar |
|---|---|---|
| `Captura_de_tela_20261103_083855.png` – `..._084105.png` | Janela vazia e menus Arquivo, Editar, Exibir, Abas, Marcadores e Ajuda | Hierarquia, agrupamento, rótulos, atalhos e comandos desabilitados sem documento |
| `Captura_de_tela_20261103_084133.png` – `..._084502.png` | Preferências, opções de interface e gráficos, mouse e atalhos | Organização por categoria, densidade dos controles, edição e persistência das opções |
| `Captura_de_tela_20261103_084545.png`, `..._084559.png` | Submenus de barras e painéis | Visibilidade de barras, miniaturas, marcadores e outros painéis |
| `Captura_de_tela_20261103_084632.png` | Diálogo Sobre | Identidade, descrição e informações de versão |
| `Captura_de_tela_20261126_220931.png` | PDF de 152 páginas aberto e menu de navegação | Barra de página e zoom, miniaturas, página centralizada, fundo de leitura e comandos de navegação |
| `Captura_de_tela_20261126_221339.png`, `..._221425.png` | Busca aberta, vazia e com consulta/resultados | Barra inferior, opções da consulta, destaque e navegação de resultados |
| `Captura_de_tela_20261126_221509.png`, `..._221642.png` | Painel de marcadores com miniaturas | Seleção, destinos, posição e largura dos painéis |
| `Captura_de_tela_20261126_221700.png` | Painel de estrutura/contorno | Lista de páginas, numeração, rolagem e navegação |
| `Captura_de_tela_20261126_221715.png` | Painel de propriedades | Apresentação de metadados e informações do arquivo |

Os intervalos de nomes acima incluem apenas os arquivos intermediários da mesma sequência de capturas, não uma afirmação de que cada captura do intervalo mostra o mesmo estado.

## Direção visual observada

- Tema escuro com menus compactos e título da janela centralizado.
- Menus na linha superior; controles do documento logo abaixo.
- Barra de navegação com página atual/total, anterior/próxima e zoom.
- Miniaturas em painel estreito à esquerda, com número e página atual identificável.
- Página PDF centralizada sobre fundo cinza e separação visível entre páginas.
- Busca na parte inferior, com opções de correspondência e controles para percorrer resultados.
- Painéis auxiliares para marcadores, estrutura e propriedades; alternância sem perder o documento ativo.
- Comandos indisponíveis devem ter estado desabilitado coerente com a ausência de documento, resultado ou histórico.

As capturas são referência de proporção e hierarquia, não uma especificação pixel a pixel. O gpdfview deve preservar comportamento nativo GTK/libadwaita, foco por teclado, acessibilidade e adaptação a diferentes tamanhos de janela.

## Limites das referências

- A captura principal usa uma tela de aproximadamente 1360 × 725 pixels e zoom de 50%; comparar em tamanho semelhante antes de ajustar dimensões.
- As imagens cobrem um único PDF de exemplo e não demonstram documentos protegidos, páginas de tamanhos mistos, muitos marcadores ou janelas estreitas.
- Não há comparação de desempenho ou de fidelidade interna de renderização apenas com screenshots.
- Opções visíveis no qpdfview não devem ser expostas como funcionais no gpdfview até que o comportamento correspondente exista.

## Roteiro de comparação do gpdfview

Capturar e revisar, em dimensões equivalentes, estes estados: janela vazia; PDF aberto com miniaturas; página atual diferente da primeira; busca vazia e com resultado; painel de marcadores; painel de estrutura; propriedades; menus com e sem documento. Conferir geometria, contraste, seleção, estados desabilitados e correspondência entre o comando exibido e sua ação.

## Comparação de implementação — 2026-11-26

Esta revisão relaciona os estados das capturas ao código atual. É uma comparação de estrutura e comportamento descritos no código; a conferência de pixels e a execução interativa ainda precisam ser feitas em uma sessão gráfica controlada.

| Estado da referência | Implementação atual | Resultado da revisão de código | Verificação restante |
|---|---|---|---|
| Janela vazia e menus | `src/window.rs`, `src/ui_commands.rs` | Menus Arquivo, Editar, Exibir, Abas, Marcadores e Ajuda; comandos dependentes de documento recebem sensibilidade conforme o PDF ativo. | Conferir largura, alinhamento, foco e estados visuais na janela real. |
| Navegação e leitura | `src/window.rs`, `src/navigation.rs`, `src/sidebar.rs` | Barra de página/zoom, fundo escuro, página clara e painel de miniaturas à esquerda; miniaturas geradas com limite de workers. | Comparar dimensões, centralização, escala e resposta ao rolar. |
| Busca e resultados | `src/window.rs`, `src/rendering.rs`, `src/sidebar.rs` | Busca recolhível na faixa inferior, opções de consulta, destaques e lista de resultados no painel auxiliar. | Conferir posição da faixa, realce sobre páginas e navegação entre ocorrências. |
| Marcadores e estrutura | `src/sidebar.rs`, `src/session.rs`, `src/engine.rs` | Abas auxiliares para contorno, marcadores do usuário e navegação por página; seleção acompanha a página atual. | Conferir recuo, seleção, rolagem e comportamento com listas longas. |
| Propriedades | `src/sidebar.rs`, `src/engine.rs`, `src/dialogs.rs` | Metadados em linhas, tamanho e caminho do arquivo no painel e na janela de propriedades. | Comparar a hierarquia visual e conferir PDFs com metadados extensos. |
| Preferências | `src/dialogs.rs` | Categorias para visualização e atalhos; opções de layout exibidas em português. | Conferir altura, rolagem, foco e legibilidade em janela estreita. |

As diferenças intencionais em relação às capturas do qpdfview são os rótulos localizados, o uso de controles GTK/libadwaita e a divisão dos conteúdos auxiliares em abas. Esta revisão não declara equivalência visual final nem validação manual da aplicação.
