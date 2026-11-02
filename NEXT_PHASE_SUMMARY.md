# Próxima fase — gpdfview

## Status atual

O projeto já está em um estado funcional e estável para a fase de fechamento técnico:

- interface GTK4/libadwaita está operacional
- renderização em worker isolado mantida em pipeline estável
- abas e documentos independentes continuam funcionando
- busca por texto, navegação e zoom estão validados
- persistência de diretório e atalhos de teclado foi adicionada
- benchmark e empacotamento já possuem scripts e referência documentada

## Verificação recente

Comando executado:

```bash
cargo test -q
```

Resultado verificado:

- 19 testes passaram
- 0 falharam
- 0 ignorados

## Foco da próxima fase

### Bloco 2 — abas independentes e ações por aba

Status verificado:

- cada aba mantém seu próprio ciclo de render e estado de navegação
- a linha de abas desaparece quando a última aba é fechada
- a gestão de títulos e fechamento segue o contrato de múltiplas abas
- a validação atual reflete que a fila de abas não está mais compartilhando estado crítico

### Bloco 3 — persistência e retomada ao abrir

- últimos PDFs e diretório atual
- restauração da última sessão válida
- startup sem regressão no fluxo de abertura

### Bloco 4 — pacote e benchmark

- manter scripts de release sob controle
- documentar uso e requisitos de execução
- archive final para uso em Linux

3. Encerramento do ciclo de projeto
   - confirmar estado funcional em smoke test
   - registrar resultados em benchmark
   - preparar a base para evolução futura

## Checklist de fechamento

- [x] base funcional estável
- [x] renderização e cache confirmados
- [x] múltiplos documentos e abas estabilizados
- [x] busca, zoom e navegação refinados
- [x] persistência do último diretório
- [x] scripts de benchmark e release
- [x] documentação de referência
- [x] validação do conjunto de testes

## Orientação final

O trabalho agora deve seguir em direção à manutenção e refinamento do que já está validado, sem regressão e sem dispersão. O objetivo é consolidar a entrega em um estado pronto para uso e para extensões futuras.
