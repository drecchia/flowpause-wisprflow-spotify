# Fases 1 e 2 — Resultados

Data do teste:
Versão do Wispr Flow:
Comando: `flowpause run` (padrões: `--resume-delay-ms 2000`, `--max-pause-secs 300`, `--match wispr`)
Estratégia de pausa ativa: CheckFirst (consulta o estado antes de pausar)

## Fase 1: detector (`flowpause watch`)

| Ditado | Linhas `ditado iniciou` | Linhas `ditado terminou` | Observação |
| --- | --- | --- | --- |
| 1 | | | |
| 2 | | | |
| 3 | | | |
| 4 | | | |
| 5 | | | |

## Fase 2: critérios de aceite 1 a 6 (seção 9 da spec)

| # | Critério | Passou? | Tempo até parar | Tempo até voltar | Observação / trecho do log |
| --- | --- | --- | --- | --- | --- |
| 1 | Música no Echo Dot: ao iniciar o ditado para em até 1 s; ao terminar, volta no Echo cerca de 2 s depois | | | | |
| 2 | Música no Spotify do PC: mesmo comportamento, com retomada no PC | | | | |
| 3 | Nada tocando: ditar não inicia música em nenhum dispositivo | | — | — | |
| 4 | Três ditados com menos de 2 s entre eles: a música para uma vez e volta uma vez, ao final do terceiro | | | | |
| 5 | Play manual durante a pausa: o programa não envia comando ao terminar o ditado | | | — | |
| 6 | Troca de dispositivo durante a pausa: o programa não retoma no dispositivo antigo | | | — | |
