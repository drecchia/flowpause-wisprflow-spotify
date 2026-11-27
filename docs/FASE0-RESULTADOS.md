# Fase 0 — Resultados

Data do teste: 27/11/2026
Versão do app Spotify no PC: não registrada
Modelo do Echo: Echo Dot ("<nome-do-echo>")

Os tempos são de chamadas a frio: cada execução da CLI abre uma conexão HTTPS nova (handshake TLS incluído).

## Echo Dot

| # | Passo | Comando | HTTP | ms | Observação |
| --- | --- | --- | --- | --- | --- |
| 1 | Login | `flowpause login` | — | — | Feito pelo `scripts\install.ps1` + login; funcionou |
| 2 | Estado com música no Echo | `flowpause status` | 200 | 1250 | `id`: `<id-do-echo>` · `is_restricted`: false · `type`: Speaker |
| 3 | Pausar | `flowpause pause` | 200 | 784 | Relato: "tudo funcionou". Tempo até o silêncio: não medido |
| 4 | Pausar de novo (H2) | `flowpause pause` | não executado | | |
| 5 | Retomar | `flowpause play --device <id>` | 200 | 631 | Relato: "tudo funcionou" |
| 6a | Pausar antes da espera | `flowpause pause` | não executado | | |
| 6b | Lista após 5 min | `flowpause devices` | não executado | | |
| 6c | Retomar após 5 min | `flowpause play --device <id>` | não executado | | |
| 6d | Transferir (só se 6c deu 404) | `flowpause transfer --device <id>` | não executado | | |

## Spotify do PC

| # | Passo | Comando | HTTP | ms | Observação |
| --- | --- | --- | --- | --- | --- |
| 2 | Estado com música no PC | `flowpause status` | 200 | 1098 | `id`: `<id-do-pc>` · `name`: <nome-do-pc> · `is_restricted`: false |
| 3 | Pausar | `flowpause pause` | não registrado | | |
| 4 | Pausar de novo (H2) | `flowpause pause` | não executado | | |
| 5 | Retomar | `flowpause play --device <id>` | não registrado | | |

Extra: `flowpause status` sem reprodução ativa devolveu HTTP 204 em 1183 ms.

## Conclusão

| Item | Resultado |
| --- | --- |
| H1: Echo aceita `pause` e `play` | Confirmada: pause 200, play 200, `is_restricted: false` |
| H2: `pause` já pausado devolve 403 | Não testada. Por isso o controlador da fase 2 consulta o estado antes de pausar |
| Tempo entre o comando e o silêncio no Echo | Não medido |
| Tempo que o Echo fica na lista depois de pausado | Não medido |
| Divergências em relação a `docs/spec.md` | Nenhuma. O pause devolveu 200 (a spec aceita 200 ou 204) |
