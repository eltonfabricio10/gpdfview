# Plano completo de execução — gpdfview

**Data-base:** 2026-11-27
**Snapshot:** branch `main`, checkout atual
**Estratégia:** melhorias incrementais, reversíveis e verificadas

## Objetivo

Transformar o gpdfview em um visualizador PDF funcional e estável para uso diário, sem regressões em navegação, abas, sessão, busca, impressão e layout.

Este plano não autoriza mudanças grandes sem validação. Cada bloco deve terminar em:

1. código revisado;
2. testes automatizados verdes;
3. build/diagnósticos verdes;
4. smoke test manual quando necessário;
5. documentação atualizada;
6. commit específico;
7. push para `main`.

## Linha de base obrigatória

Antes de cada bloco:

```bash
cd /home/elton/Downloads/gpdfview
git status --short
git pull --ff-only origin main
cargo fmt --all -- --check
cargo test -q
cargo clippy -q --all-targets --all-features
```

Se a toolchain estiver ausente do PATH:

```bash
source ~/.cargo/env 2>/dev/null || true
rustup show active-toolchain
```

Nenhum bloco começa com o repositório sujo ou com uma suíte já falhando. Falhas preexistentes devem ser registradas e isoladas.

## Política anti-regressão

### Sempre preservar

- abertura local de PDF;
- primeira página renderizada rapidamente;
- página única, contínua, duas páginas e divisão;
- navegação por página e resultados de busca;
- miniaturas, sumário e marcadores;
- favoritos e documentos recentes;
- estado por documento;
- geometria e estado da janela;
- atalho configurável;
- impressão direta e fallback;
- comportamento de fechamento/restauração de abas.

### Gates por mudança

Toda alteração deve passar por:

- teste unitário do módulo afetado;
- teste de integração dos fluxos adjacentes;
- teste de cancelamento ou stale generation quando houver worker;
- teste de dois documentos ao menos;
- teste de fechamento/reabertura de aba;
- validação manual em Wayland para ações GTK sensíveis;
- ausência de warnings novos em clippy.

### Separação de commits

Cada bloco deve ser dividido em commits menores sempre que houver mudanças independentes. Cada commit deve conter:

- uma responsabilidade;
- uma justificativa clara;
- testes relacionados;
- documentação mínima quando alterar comportamento do usuário.

## Convenção de commits

- `fix: ...` — correção de bug;
- `feat: ...` — recurso novo;
- `refactor: ...` — mudança interna sem mudança funcional;
- `test: ...` — cobertura ou regressão;
- `perf: ...` — melhoria de desempenho;
- `docs: ...` — documentação;
- `chore: ...` — ferramenta ou infraestrutura.

Convenção final:

```text
<tipo>: <ação objetiva em português>
```

Exemplo:

```text
fix: evitar ação global usar controller inativo
```

## Bloco 1 — estabilidade de arquitetura e abas

### Objetivo

Garantir que toda ação global atua no documento ativo e que abas podem ser alternadas, reordenadas, fechadas e restauradas sem trocar de PDF.

### Tarefas

1. Centralizar resolução do controller ativo em `DocumentTabManager::active_controller()`.
2. Remover fallback divergente dos menus/toolbar que possa apontar para o controller inicial.
3. Garantir que imprimir usa exatamente uma conexão e o controller ativo.
4. Vincular título, status, sidebar e botão de impressão a `switch-page`.
5. Preservar metadata estável por identidade da página/box GTK.
6. Revisar reorder → close → restore.
7. Testar todas as abas fornecidas a `GApplication::open`.
8. Corrigir mensagens de worker indisponível e falha de criação de controller.

### Testes

- duas ou mais abas carregadas;
- reordenação seguida de fechamento;
- restauração da última aba fechada;
- fechar aba ativa e não ativa;
- imprimir cada documento em sequência;
- sidebar mudar de documento;
- abrir vários PDFs via picker e GApplication;
- fechar todas as abas e evitar estado antigo residual.

### Critérios de saída

- nenhum menu global usa controller inicial sem fallback explícito;
- nenhuma abertura usa só `files.first()`;
- reordenação não altera título ou path;
- estado vazio é apresentado ao fechar a última aba;
- workers são cancelados ou liberados na troca/fechamento.

### Validação

```bash
cargo fmt --all -- --check
cargo test -q
git diff --check
cargo clippy -q --all-targets --all-features
```

### Commits sugeridos

1. `fix: centralizar resolucao do documento ativo`
2. `fix: sincronizar sidebar e titulo ao trocar aba`
3. `fix: validar abertura multipla e restauracao de abas`
4. `test: cobrir reorder, close e restore de documentos`

Push após cada commit estável.

## Bloco 7 — desempenho e limites Poppler

### Objetivo

Evitar bloqueio da interface em PDFs grandes e tornar cancelamento, cache e workers previsíveis.

### Tarefas

1. Manter page rendering prioritário sobre prefetch/thumbnail/outline.
2. Revisar todo acesso a `poppler::Document`, `Page` e FFI.
3. Preservar o mutex global somente onde a biblioteca exigir serialização.
4. Garantir que search, outline, propriedades e thumbnails sempre verifiquem geração antes de publicar resultado.
5. Liberar o mutex entre páginas de buscas grandes.
6. Medir/limitar população de miniaturas e rows.
7. Evitar que worker por aba cresça sem limites em sessões longas.
8. Expor métricas internas de cache sem afetar a interface.

### Testes

- navegação rápida para frente e para trás;
- cancelar busca antiga;
- trocar PDF durante render em curso;
- fechar documento durante thumbnail/search;
- páginas gigantes ou imagem pesada;
- PDF malformado e inválido;
- PDF com milhares de páginas.

### Critérios de saída

- primeira página não espera toda a sidebar;
- render stale não substitui página atual;
- cache permanece dentro dos limites;
- UI continua respondendo durante search/thumbnail;
- worker não mantém documento fechado vivo;
- erro de Poppler não derruba o processo.

### Commits sugeridos

1. `perf: priorizar pagina visivel sobre prefetch`
2. `fix: cancelar resultados stale de metadados e busca`
3. `perf: limitar e acompanhar cache de paginas e miniaturas`
4. `test: cobrir worker, cancelamento e PDFs grandes`

## Bloco 2 — visualização e navegação

### Objetivo

Estabilizar os modos de leitura e seus limites em documentos reais.

### Tarefas

1. Testar páginas únicas de tamanho misto.
2. Corrigir altura/largura/stride do modo contínuo.
3. Ancorar scroll ao trocar zoom ou layout.
4. Definir alinhamento de página dupla para pares/ímpares.
5. Preservar página atual ao sair e retornar ao contínuo.
6. Validar split horizontal/vertical e sincronização.
7. Completar roda do mouse sem alterar zoom indevidamente.
8. Cobrir roda, teclado, fit width, fit page e rotação.

### Testes

- A4, Letter, paisagem e páginas misturadas;
- página única, contínua, duas páginas e split;
- zoom 10%–400%;
- rotação 90/180/270;
- resize da janela;
- scroll até primeira/última página;
- busca cruzando páginas;
- PDF do Google Docs com imagens grandes.

### Critérios de saída

- nenhuma página aparece em área vazia gigantesca;
- nenhuma página fica cortada ou deslocada no scroll;
- fit page considera a viewport real;
- página visível sincroniza com miniaturas;
- Ctrl+roda altera zoom e roda sem Ctrl faz scroll.

### Commits sugeridos

1. `fix: normalizar geometria dos layouts`
2. `fix: ancorar scroll continuo ao trocar zoom`
3. `fix: alinhar paginas duplas e misturadas`
4. `test: cobrir layouts, zoom e rotacao`

## Bloco 3 — gestão completa de documentos e sessão

### Objetivo

Completar abertura, fechamento, duplicidade e restauração de documentos.

### Tarefas

1. Abrir todos os arquivos enviados por GApplication.
2. Aplicar política clara para repetir o mesmo PDF em uma nova aba.
3. Persistir ordem das abas e estado por documento.
4. Restaurar múltiplas abas na inicialização, respeitando o produto documentado.
5. Detectar arquivo deletado, inacessível ou alterado externamente.
6. Remover recentes/favoritos inválidos com UX clara.
7. Garantir release dos worker pair ao fechar aba.
8. Separar “nenhuma aba aberta” de “último PDF fechado” no resume.

### Testes

- dez PDFs em abas;
- fechar/reabrir/restaurar todas;
- reiniciar app;
- arquivo renomeado/deletado;
- arquivo aberto por linha de comando;
- múltiplos arquivos passados ao launcher;
- restauração de zoom/rotação/layout por PDF.

### Critérios de saída

- nenhuma sessão restaura PDF errado;
- restauração não cria aba vazia;
- abrir vários PDFs cria uma aba por arquivo válido;
- arquivo inválido recebe feedback localizado;
- estado da última aba é salvo sem depender de close-tab legado.

### Commits sugeridos

1. `fix: abrir todos os arquivos recebidos pela aplicacao`
2. `feat: persistir ordem e restaurar sessoes de abas`
3. `fix: detectar arquivos recentes inacessiveis`
4. `test: cobrir sessao multipla e reinicializacao`

## Bloco 4 — sidebar e estrutura documental

### Objetivo

Fazer miniaturas, sumário, favoritos e marcadores serem responsivos em documentos extensos.

### Tarefas

1. Virtualizar a lista/grid de miniaturas.
2. Usar fila assíncrona priorizando páginas visíveis.
3. Preservar cache separado de 24 MiB.
4. Mostrar estado vazio, loading, erro e cancelamento.
5. Tornar outline assíncrona e robusta a ações não suportadas.
6. Navegar somente destinos válidos ou desativar entradas perigosas.
7. Sincronizar seleção de miniatura com a página visível.
8. Opcionalmente persistir largura/visibilidade dos painéis.

### Testes

- documento com 477 páginas;
- scroll rápido na lista de miniaturas;
- troca de PDF com request antigo;
- outline vazia, aninhada e com destinos inválidos;
- favoritos, recentes e marcadores da aba ativa;
- erro de imagem/fontes no documento.

### Critérios de saída

- poucas widgets são criadas por vez;
- nenhum PDF antigo publica miniaturas na aba nova;
- outline não bloqueia a interface;
- seleção de miniatura navega corretamente;
- estados vazios não contam como erros.

### Commits sugeridos

1. `perf: virtualizar navegação de miniaturas`
2. `fix: cancelar thumbnails stale ao trocar documento`
3. `feat: robustecer carregamento do sumario PDF`
4. `test: cobrir sidebar em documentos longos`

## Bloco 5 — busca e seleção textual

### Objetivo

Fazer busca documental rápida, cancelável e inteligível.

### Tarefas

1. Corrigir label para indicar busca no documento.
2. Buscar em worker com progresso incremental.
3. Limitar armazenamento sem silenciar limite alto.
4. Mostrar contagem parcial/total e página atual.
5. Navegar próximo/anterior sem perda de resultado.
6. Preservar highlights por página.
7. Cancelar por query/documento/aba alterado.
8. Adicionar seleção/cópia textual se viável.

### Testes

- documento sem texto;
- documento escaneado;
- 1.000+ páginas;
- query vazia, Unicode e acentos;
- mudar query durante scan;
- trocar aba durante scan;
- muitos matches na mesma página;
- cancelar e reiniciar rapidamente.

### Critérios de saída

- nenhuma busca antiga sobrescreve uma busca nova;
- a UI continua responsiva;
- próximo/anterior é estável;
- resultados são limitados e informados;
- highlight não aparece na página errada.

### Commits sugeridos

1. `fix: tornar busca documental cancelavel`
2. `feat: mostrar progresso e contagem de matches`
3. `fix: isolar highlight por documento e pagina`
4. `test: cobrir busca longa e stale generations`

## Bloco 6 — impressão profissional

### Objetivo

Entregar impressão previsível em CUPS/IPP e fallback Cairo.

### Tarefas

1. Corrigir possibilidade de escala dupla no fallback.
2. Adicionar seleção de impressora se disponível pela API CUPS/GTK.
3. Excluir escala interna do fallback ou do `lp`, escolhendo um único estágio.
4. Validar faixa, cópias, orientação e escala.
5. Evitar PDF fallback completo em memória.
6. Remover temporários em sucesso e falha.
7. Localizar todos os resultados e erros.
8. Testar fila CUPS real ou fila de teste do ambiente.

### Testes

- todas páginas;
- página atual;
- faixa;
- retrato/paisagem;
- 50/100/200%;
- cópias;
- PDF protegido ou complexo;
- falha de impressora;
- fallback desativado/indisponível.

### Critérios de saída

- geometria correta em test queue;
- escala aplicada exatamente uma vez;
- temporários não são deixados;
- documento ativo é impresso;
- erro não desativa o botão permanentemente.

### Commits sugeridos

1. `fix: aplicar escala de impressao uma unica vez`
2. `feat: selecionar destino CUPS disponivel`
3. `fix: limpar temporarios e erros de impressao`
4. `test: validar pagina, copia, orientacao e escala`

## Bloco 8 — UX e configurações

### Objetivo

Unificar menus, toolbar, atalhos e feedback em uma interface coerente.

### Tarefas

1. Usar apenas um dispatcher de comandos para ações visíveis.
2. Preservar atalhos customizados e atualizar hints.
3. Concluir grupos de preferências sem configuração ambígua.
4. Melhorar propriedades, status e estados vazios.
5. Validar foco, teclado, tipografia e acessibilidade básica.
6. Revisar janelas estreitas e DPI alto.
7. Confirmar visibilidade e sensibilidade dos controles por documento ativo.

### Testes

- todas as 12 ações com atalhos;
- conflito de binding;
- binding desativado;
- cancelamento de diálogo;
- erro de escrita da sessão;
- estado sem documento;
- estado com uma/muitas abas;
- navegação só por teclado.

### Critérios de saída

- todo botão visível executa ação real;
- alteração de shortcut atualiza hints;
- preferências inválidas não são persistidas;
- foco e loading são claros;
- PT-BR consistente.

### Commits sugeridos

1. `refactor: unificar dispatcher de comandos da UI`
2. `fix: concluir validacao de atalhos e preferencias`
3. `fix: melhorar estados vazios e feedback acessivel`
4. `test: cobrir preferencia, teclado e sensibilidade`

## Bloco 9 — segurança e robustez

### Objetivo

Tratamento seguro de PDFs não confiáveis e falhas de recurso.

### Tarefas

1. Implementar prompt de senha PDF.
2. Definir limites por página, arquivo, cache e worker.
3. Criar estratégia para PDF truncado/corrompido.
4. Validar URI, path, temporary path e nomes locais.
5. Evitar executar ações externas originadas em conteúdo PDF.
6. Interromper trabalho e limpar recursos em worker falho.
7. Adicionar testes de PDF inválido, enorme e protegido.

### Testes

- senha correta/incorreta/cancelada;
- truncado no meio;
- dimensões absurdas;
- compressão enorme;
- imagens gigantes;
- links externos;
- filesystem cheio;
- permissão negada;
- terminação inesperada do worker.

### Critérios de saída

- nenhum crash ou OOM controlável;
- mensagem clara para senha/arquivo inválido;
- temporários são limpos;
- links externos são tratados pelo sistema seguro;
- resource limit é aplicado antes da alocação.

### Commits sugeridos

1. `feat: adicionar abertura segura de PDFs protegidos`
2. `fix: limitar paginas, buffers e temporarios extremos`
3. `fix: recuperar falha de worker e limpar recursos`
4. `test: cobrir PDFs malformados e protecao por senha`

## Bloco 10 — testes, benchmarks e release

### Objetivo

Produzir evidência objetiva antes de afirmar estabilidade ou paridade.

### Tarefas

1. Substituir benchmark que usa wrapper `timeout`.
2. Medir processo gpdfview real e peak RSS.
3. Medir document parse → primeira página renderizada.
4. Medir renderizada → apresentada na tela.
5. Medir navegação sequencial e aleatória.
6. Medir cache, idle, preload e impressão.
7. Rodar comparação com qpdfview sob o mesmo ambiente.
8. Completar matriz real de regressão GUI.
9. Revisar licença e dependências Poppler nativas.
10. Atualizar desktop entry, ícone, empacotamento e release notes.
11. Congelar e testar matriz final.

### Benchmark mínimo

- hardware registrado;
- display server registrado;
- versões GTK, libadwaita, Poppler, Cairo e Rust;
- cold e warm runs;
- small, medium, medium-long e large-long;
- cada documento repetido pelo menos três vezes;
- first rendered, first presented, peak RSS e idle RSS;
- navegação sequencial e aleatória;
- zoom fit e impressão.

### Critérios de release

- sem P0 aberto;
- sem crash reproduzível;
- testes e clippy verdes;
- benchmarks documentados;
- licença revisada;
- artefatos empacotados;
- matriz de features atualizada;
- README e docs alinhados ao comportamento real.

### Commits sugeridos

1. `test: substituir benchmark por medicao real de processo`
2. `test: adicionar corpus de PDFs adversarios`
3. `docs: publicar resultados reproduziveis e ambiente`
4. `chore: revisar licenciamento e artefatos de release`

## Sequência recomendada de execução

1. Bloco 1.
2. Bloco 7.
3. Bloco 2.
4. Bloco 3.
5. Bloco 4.
6. Bloco 5.
7. Bloco 6.
8. Bloco 8.
9. Bloco 9.
10. Bloco 10.

Razão: estabilidade de abas e ownership vêm antes da expansão de workers; leitura vem antes da expansão de sessão/sidebar; segurança e release só devem fechar depois das funcionalidades básicas já estáveis.

## Procedimento único por commit

```bash
cargo fmt --all
cargo fmt --all -- --check
cargo test -q
cargo clippy -q --all-targets --all-features
git diff --check
git status --short
git add -A
git diff --cached --stat
git diff --cached --check
git commit -m "<mensagem>"
git push origin main
```

Para mudanças que alteram documentação ou paridade:

```bash
cargo test -q
git diff --check
git add docs/ <arquivos-fonte>
git commit -m "docs: atualizar status do bloco"
git push origin main
```

## Fluxo após cada bloco

Ao concluir um bloco:

1. executar todos os comandos de validação;
2. registrar checks manuais que não puderam ser feitos;
3. atualizar `docs/FEATURE_PARITY.md`;
4. atualizar esta matriz apenas se o plano mudou;
5. criar commit de implementação;
6. criar commit de testes separadamente se fizer sentido;
7. fazer push;
8. registrar hash do merge principal do bloco;
9. iniciar o próximo bloco somente com baseline limpa.

## Evidências exigidas

Para concluir o plano, o repositório deve conter:

- status de build atual;
- link de cada commit/push por bloco;
- lista de PDFs testados;
- lista de cenários GTK testados;
- hardware, SO e versões das bibliotecas;
- resultados de benchmark;
- bugs conhecidos separados de P0/P1/P2;
- licença e dependências revisadas;
- artefato release executável.
