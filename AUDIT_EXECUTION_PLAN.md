# Auditoria e execução por blocos

## Escopo e método

Auditoria estática de renderização, navegação, cache, busca, persistência, abas, impressão e scripts. Alterações preexistentes: árvore limpa no início. Cada bloco terá commit próprio sem trailers de atribuição de IA. Push normal para origin/main, sem force; falhas de rede/autenticação serão registradas, sem alegar publicação.

## Bloco 1 — estabilidade e limites

Corrigir empréstimo de RefCell vivo durante notificação de erro de busca; preservar página inicial enquanto a contagem é desconhecida e ajustar à contagem real no worker; restaurar layout salvo; aceitar renderização com escala adaptativa e exibir seu zoom efetivo; limitar dimensões de miniaturas gigantes. Adicionar regressões para miniaturas, retomada e documento encurtado.

## Bloco 2 — cache e persistência

Guardar metadados junto à superfície e compartilhar a finalização de apresentação entre cache e worker, incluindo persistência, scroll pendente e solicitação de páginas complementares. Promover documentos reabertos nos recentes e salvar documento realmente ativo ao fechar a janela.

## Bloco 3 — temporários e distribuição

Reservar diretório temporário privado com criação exclusiva e limpeza RAII; testar unicidade, permissões e cleanup. Corrigir pacote para usar build --locked e target-dir explícito, resolver destino relativo antes de mudar diretório e usar arquitetura real no nome. Exercitar com Cargo simulado e falha de build.

## Bloco 4 — revisão e evidências

Revisar diff de forma independente, verificar whitespace e sintaxe dos scripts, executar testes disponíveis e conferir histórico/estado Git. Compilação, testes Rust, lint e GUI exigem ferramentas ausentes neste ambiente e não serão declarados aprovados sem execução.

## Achados adicionais e plano subsequente

A geometria contínua supõe páginas uniformes e precisa de posições cumulativas por página, usadas pelo canvas, desenho e seleção por scroll. Os modos de duas páginas e split precisam compor dimensões reais de ambas as páginas; fit deve usar esse bounding box. A correção deve ser um bloco arquitetural próprio, testado com páginas de tamanhos mistos e milhares de alturas fracionárias.

A transformação dos resultados de busca precisa considerar rotação e origem da página no canvas. A navegação explícita em modo contínuo precisa se distinguir de mudanças originadas por scroll. Validar quatro rotações, cache quente/frio e páginas distantes antes de publicar essa mudança.

O ciclo de vida da impressão pode descartar trabalhos enfileirados ao fechar uma aba e interromper fallback ao sair. A solução demanda serviço de aplicação, retenção da aplicação durante trabalhos aceitos e encerramento coordenado, com subprocesso lp controlado; não basta bloquear a thread GTK com join.

A persistência delimitada por linhas/tabulações não preserva todos os nomes Linux. Migrar com compatibilidade para formato que escape delimitadores e preserve bytes, incluindo testes de caminhos não UTF-8; evitar alterar silenciosamente o formato sem migração.

O benchmark usa PID de uma instância potencialmente secundária e detecção de janela X11 mesmo aceitando Wayland. Isolar D-Bus/dados de sessão e instrumentar primeira página no aplicativo em vez de tratar xdotool como medição universal.

O worker pode concluir trabalho obsoleto e o canal de respostas não possui orçamento de bytes. Introduzir verificação de geração antes e depois do render e backpressure sem bloquear shutdown; medir RSS e latência sob navegação rápida.

## Limites

Análise estática não substitui validação gráfica. A lista de achados não constitui garantia de ausência de outros bugs. Os blocos arquiteturais acima permanecem pendentes até implementação e testes específicos.
