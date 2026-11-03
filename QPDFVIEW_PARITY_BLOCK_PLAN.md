# Plano Bloco a Bloco para Paridade com qpdfview

> **Historical snapshot — 2026-11-03 audit note:** Historical working plan. Its feature descriptions and architectural observations are not authoritative where they conflict with the audited source. Use `docs/FEATURE_PARITY.md`, `docs/ARCHITECTURE.md` and `docs/IMPLEMENTATION_PLAN.md`.


Data: 2026-11-02
Base de analise: leitura de `sources/mainwindow.*` e `sources/documentview.*` do qpdfview + estado atual do gpdfview.

Objetivo: reproduzir comportamento observavel do qpdfview no gpdfview sem copiar codigo literal.

## Resumo do estado atual do gpdfview

Ja existe:
- Abertura de PDF, abas, navegacao, zoom, fit width/page, rotacao.
- Layout single/continuous/two-page.
- Busca no documento inteiro com next/previous e contador global.
- Outline (bookmarks do PDF) e miniaturas reais com cache dedicado.
- Persistencia por documento (pagina, zoom, rotacao, layout).
- Impressao com caminho PDF direto preferencial e fallback renderizado com opcoes.
- Barra de menu visual classica (arquivo/editar/exibir/abas/marcadores/ajuda) e sidebar no estilo miniaturas.

Pontos fracos atuais:
- Menus ainda sao botoes simples (nao menus hierarquicos reais com atalhos e estados).
- Sidebar usa polling periodico para refresh (deve migrar para eventos/sinais).
- Docking e split view do qpdfview ainda nao existem.
- Fluxos avancados de busca (painel de resultados/contexto) nao existem.
- Bookmarks internos do app (nao outline PDF) ainda limitados.
- Configuracoes completas (settings dialog) e atalhos configuraveis ainda ausentes.

## Bloco 0 - Higiene de base antes da paridade

Ja tem:
- Testes unitarios e clippy em estado verde.

Melhorar:
- Trocar polling da sidebar por atualizacao dirigida por evento.
- Normalizar arquitetura de comandos UI para reduzir acoplamento da `window.rs`.

Adicionar:
- Mapa de comandos (`UiCommand`) com roteamento unico para botoes, menu, atalhos.
- Hooks de telemetria local para tempos de render, busca e impressao.

Criterio de aceite:
- Sem timer de polling para atualizar miniaturas/outline.
- Mesma acao acionavel por botao, menu e atalho.

## Bloco 1 - MainWindow fiel (menus e toolbar classicos)

Referencia qpdfview:
- `QMenuBar` + `QToolBar` + acao contextual por estado de aba.

Ja tem:
- Header, barra de menu visual e toolbar funcional parcial.

Melhorar:
- Menus reais com submenus, separadores e estado enabled/disabled.
- Toolbar compacta no estilo qpdfview (ordem e densidade).

Adicionar:
- Menus reais: Arquivo/Editar/Exibir/Abas/Marcadores/Ajuda com itens completos.
- Atalhos padrao conectados a mesmas acoes de menu.
- Politica de habilitacao por contexto (sem documento, com documento, em busca ativa etc.).

Criterio de aceite:
- Fluxos principais acionaveis por menu e atalho.
- UI visualmente equivalente em densidade e ordem de controles.

## Bloco 2 - Sidebars como docks (Miniaturas, Outline, Marcadores, Busca)

Referencia qpdfview:
- Docks independentes, alternaveis e persistiveis.

Ja tem:
- Sidebar unica com miniaturas, outline, recentes e favoritos.

Melhorar:
- Separar responsabilidades em paineis dedicados.
- Sincronizar selecao da miniatura com pagina visivel de forma precisa.

Adicionar:
- Docks/paineis independentes (Miniaturas, Outline, Marcadores, Busca).
- Toggle por menu Exibir.
- Persistencia de visibilidade e posicao dos paineis.

Criterio de aceite:
- Usuario controla visibilidade de cada painel como no qpdfview.
- Miniaturas e outline sincronizam automaticamente com a pagina atual.

## Bloco 3 - Busca avancada (dock de busca e resultados)

Referencia qpdfview:
- `SearchModel`, progresso, contexto e navegacao por resultados.

Ja tem:
- Busca global por documento, contador e highlight.

Melhorar:
- Exibir lista de resultados com contexto (trecho) e pagina.
- Indicador de progresso de busca.

Adicionar:
- Painel de resultados de busca.
- Busca por aba atual e opcao para todas as abas abertas.
- Opcoes match-case/whole-word e cancelamento robusto.

Criterio de aceite:
- Resultado clicavel leva ao trecho correto.
- Progresso e cancelamento funcionam sem congelar UI.

## Bloco 4 - Gestao de abas completa

Referencia qpdfview:
- Recentemente fechadas, restaurar aba, menu de contexto rico.

Ja tem:
- Abertura/fechamento basico de abas.

Melhorar:
- Politicas de fechamento em lote.
- Melhor tratamento de foco e restauracao.

Adicionar:
- Historico de abas fechadas + restaurar ultima.
- Context menu de abas (fechar outras, esquerda, direita, etc.).
- Navegacao de abas por atalho (prev/next).

Criterio de aceite:
- Conjunto de acoes de abas equivalente ao essencial do qpdfview.

## Bloco 5 - Split View real

Referencia qpdfview:
- Split horizontal/vertical por aba e sincronizacao opcional.

Ja tem:
- Nao ha split view real.

Melhorar:
- Modelo de estado por pane para evitar mistura de navegacao.

Adicionar:
- Split horizontal e vertical.
- Fechar pane atual.
- Opcao de sincronizar navegacao/zoom entre panes.

Criterio de aceite:
- Dois viewers simultaneos no mesmo documento sem conflito de estado.

## Bloco 6 - Marcadores do aplicativo (nao apenas outline do PDF)

Referencia qpdfview:
- BookmarkModel/BookmarkMenu com acoes por arquivo/pagina.

Ja tem:
- Favoritos de arquivo.

Melhorar:
- Diferenciar favoritos de arquivo vs marcadores de pagina.

Adicionar:
- Marcadores por documento/pagina com comentario.
- Navegar para marcador, remover individual/todos.
- Integracao com menu Marcadores.

Criterio de aceite:
- Marcadores persistem e sao navegaveis por menu e painel.

## Bloco 7 - Configuracoes completas (Settings dialog)

Referencia qpdfview:
- Configuracoes extensas de visualizacao, comportamento e atalhos.

Ja tem:
- Persistencias pontuais (diretorio, janela, estado por documento).

Melhorar:
- Centralizar configuracoes em estrutura unica versionada.

Adicionar:
- Dialogo de configuracoes.
- Opcoes de layout default, zoom, render flags, comportamento de abas/docks.
- Atalhos configuraveis.

Criterio de aceite:
- Alteracoes persistem e aplicam sem regressao entre sessoes.

## Bloco 8 - Impressao de nivel qpdfview

Referencia qpdfview:
- Integracao com `PrintDialog`, opcoes detalhadas e comportamento por range.

Ja tem:
- Opcoes base (range/copias/orientacao/escala), PDF direto preferencial.

Melhorar:
- UX de impressao e mensagens de fallback.
- Mapeamento de opcoes para CUPS mais abrangente.

Adicionar:
- Dialogo de impressao mais completo (intervalos custom, current page, all, quality).
- Relatorio claro de caminho escolhido (direto/fallback) e tempos.

Criterio de aceite:
- Opcoes aplicadas corretamente nos dois caminhos.

## Bloco 9 - Recursos avancados (fase 2)

Adicionar (apenas apos estabilizar 0-8):
- Anotacoes e widgets de formulario (escopo por prova tecnica).
- SyncTeX (se publico alvo exigir).
- Arquitetura de plugins para formatos adicionais (DjVu/PS/imagem), inspirada no plugin handler do qpdfview.

## Ordem de implementacao recomendada

1. Bloco 0 (higiene/eventos/comandos)
2. Bloco 1 (menus/toolbar reais)
3. Bloco 2 (docks/paineis)
4. Bloco 3 (busca avancada)
5. Bloco 4 (abas completas)
6. Bloco 5 (split view)
7. Bloco 6 (marcadores do app)
8. Bloco 7 (settings)
9. Bloco 8 (impressao avancada)
10. Bloco 9 (fase 2)

## Regra de entrega por bloco

- Implementar vertical slice completa.
- Adicionar testes de regressao.
- Rodar `cargo test -q` e `cargo clippy -q --all-targets --all-features`.
- Commit separado por bloco.
- Atualizar matriz de paridade ao final de cada bloco.
