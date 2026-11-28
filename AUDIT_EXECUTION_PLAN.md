# Auditoria e execução por blocos

## Resultado da execução

| Bloco | Status | Commit | Arquivos |
|-------|--------|--------|----------|
| 1 — estabilidade e limites | Concluído | c37f08c | src/app.rs, src/engine.rs, src/rendering.rs, src/viewer_state.rs |
| 2 — cache e persistência | Concluído | 7bb4a6d | src/app.rs, src/rendering.rs, src/session.rs, src/window.rs |
| 3 — temporários e distribuição | Concluído (sub-agente) | parte de c37f08c | src/printing.rs, scripts/package_release.sh, scripts/test_package_release.sh |
| 4 — revisão e evidências | Em andamento | — | AUDIT_EXECUTION_PLAN.md |

## Limitações do ambiente

O FUSE do sandbox impede remoção de arquivos em `.git/`, bloqueando operações git no repositório montado. As alterações foram commitadas em repositório separado (`/tmp/gpdfview-work`) e sincronizadas de volta ao repositório original. O push para origin/main retornou HTTP 403 do proxy — commit local válido sem atribuição de IA.

## Bloco 1 — estabilidade e limites

Corrigido empréstimo de RefCell vivo durante notificação de erro de busca; página inicial preservada enquanto contagem é desconhecida e ajustada à contagem real no worker; layout salvo restaurado; renderização com escala adaptativa exibindo seu zoom efetivo; dimensões de miniaturas gigantes limitadas. Adicionadas regressões para miniaturas, retomada e documento encurtado.

## Bloco 2 — cache e persistência

Metadados armazenados junto à superfície em HashMap separado. Hits de cache agora restauram page_width_points, page_height_points, page_count e atualizam zoom, spin, layout toggle e status. Recentes reordenados para MRU (mais recente primeiro). Documento ativo salvo corretamente ao fechar a janela. Layout_mode persistido e restaurado.

## Bloco 3 — temporários e distribuição

TemporaryPdf retorna Result, cria diretório exclusivo com modo 0700, limpeza RAII. Pacote usa --locked e --target-dir explícito, destino relativo resolvido antes de cd, arquitetura via uname -m. Testes shell com Cargo simulado: 4 cenários passaram.

## Bloco 4 — revisão e evidências

### Limitações de validação

Rust/Cargo ausentes — testes de unidade não puderam ser executados. `git diff --check` passou no repositório separado. `bash -n` passou para scripts. Compilação, testes Rust, lint (clippy/rustfmt) e validação GUI exigem toolchain não disponível.

### Checklist de integridade

- [x] `apply_render_request`: preserva página salva até page_count > 0
- [x] `render_page`: clampa page_index ao comprimento do documento
- [x] `thumbnail_scale`: limita max_edge a 1024px, valida dimensões
- [x] `choose_render_scale`: margem 0.98, escala adaptativa quando solicitado excede orçamento
- [x] `present_page`: armazena CachedPageMetadata junto com superfície no cache
- [x] Cache hit: restaura metadados, atualiza UI (zoom, spin, total, layout toggle)
- [x] `normalize_recent_paths`: ordem MRU correta (removeu reverse)
- [x] `open_document`: restaura layout_mode salvo
- [x] `clear_document_after_close`: limpa page_metadata
- [x] `TemporaryPdf::create`: retorna Result, diretório exclusivo, permissões 0700
- [x] `package_release.sh`: --locked, --target-dir, uname -m, destino relativo antes cd
- [x] Testes shell: 4 cenários passaram (relativo, absoluto, padrão, falha)

### Push

Tentativa origin/main bloqueada por proxy (HTTP 403). Commit local válido com associação eltonff/eltonfabricio10@gmail.com.

## Achados adicionais e plano subsequente

A geometria contínua supõe páginas uniformes e precisa de posições cumulativas por página, usadas pelo canvas, desenho e seleção por scroll. Os modos de duas páginas e split precisam compor dimensões reais de ambas as páginas; fit deve usar esse bounding box. A correção deve ser um bloco arquitetural próprio, testado com páginas de tamanhos mistos e milhares de alturas fracionárias.

A transformação dos resultados de busca precisa considerar rotação e origem da página no canvas. A navegação explícita em modo contínuo precisa se distinguir de mudanças originadas por scroll. Validar quatro rotações, cache quente/frio e páginas distantes antes de publicar essa mudança.

O ciclo de vida da impressão pode descartar trabalhos enfileirados ao fechar uma aba e interromper fallback ao sair. A solução demanda serviço de aplicação, retenção da aplicação durante trabalhos aceitos e encerramento coordenado, com subprocesso lp controlado; não basta bloquear a thread GTK com join.

A persistência delimitada por linhas/tabulações não preserva todos os nomes Linux. Migrar com compatibilidade para formato que escape delimitadores e preserve bytes, incluindo testes de caminhos não UTF-8; evitar alterar silenciosamente o formato sem migração.

O benchmark usa PID de uma instância potencialmente secundária e detecção de janela X11 mesmo aceitando Wayland. Isolar D-Bus/dados de sessão e instrumentar primeira página no aplicativo em vez de tratar xdotool como medição universal.

O worker pode concluir trabalho obsoleto e o canal de respostas não possui orçamento de bytes. Introduzir verificação de geração antes e depois do render e backpressure sem bloquear shutdown; medir RSS e latência sob navegação rápida.
