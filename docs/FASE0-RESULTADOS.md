# Fase 0 — Resultados

Data do teste:
Versão do app Spotify no PC:
Modelo do Echo:

## Echo Dot

| # | Passo | Comando | HTTP | ms | Observação |
| --- | --- | --- | --- | --- | --- |
| 1 | Login | `flowpause login` | | | |
| 2 | Estado com música no Echo | `flowpause status` | | | `id`: · `is_restricted`: |
| 3 | Pausar | `flowpause pause` | | | Echo parou? Tempo até o silêncio: |
| 4 | Pausar de novo (H2) | `flowpause pause` | | | |
| 5 | Retomar | `flowpause play --device <id>` | | | Voltou no Echo? |
| 6a | Pausar antes da espera | `flowpause pause` | | | |
| 6b | Lista após 5 min | `flowpause devices` | | | Echo ainda aparece? |
| 6c | Retomar após 5 min | `flowpause play --device <id>` | | | |
| 6d | Transferir (só se 6c deu 404) | `flowpause transfer --device <id>` | | | |

## Spotify do PC

| # | Passo | Comando | HTTP | ms | Observação |
| --- | --- | --- | --- | --- | --- |
| 2 | Estado com música no PC | `flowpause status` | | | `id`: · `is_restricted`: |
| 3 | Pausar | `flowpause pause` | | | PC parou? |
| 4 | Pausar de novo (H2) | `flowpause pause` | | | |
| 5 | Retomar | `flowpause play --device <id>` | | | Voltou no PC? |

## Conclusão

| Item | Resultado |
| --- | --- |
| H1: Echo aceita `pause` e `play` | |
| H2: `pause` já pausado devolve 403 | |
| Tempo entre o comando e o silêncio no Echo | |
| Tempo que o Echo fica na lista depois de pausado | |
| Divergências em relação a `docs/spec.md` | |
