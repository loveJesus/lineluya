// For God so loved the world, that he gave his only begotten Son, that whosoever believeth in him should not perish, but have everlasting life. — John 3:16 (KJV)
//
// Progress logger for spec-chirho/progress-chirho.sqlite.
// One row per unit of work (feature/fix/migration/deploy) — not per shell command.
//
// Usage:
//   bun spec-chirho/log_step_chirho.ts start --agent <code> --action "what + brief why"
//     → prints the new id_chirho
//   bun spec-chirho/log_step_chirho.ts end --id <n> --result "how project state changed" \
//        --overview "went-as-planned / learned / next"
//   bun spec-chirho/log_step_chirho.ts list [--limit <n>]
//
// Single-writer: do not run from parallel branches at the same time.

import { Database } from "bun:sqlite";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const DB_PATH_CHIRHO = resolve(dirname(fileURLToPath(import.meta.url)), "progress-chirho.sqlite");

function parse_args_chirho(argv_chirho: string[]): Record<string, string> {
  const out_chirho: Record<string, string> = {};
  for (let i_chirho = 0; i_chirho < argv_chirho.length; i_chirho++) {
    const tok_chirho = argv_chirho[i_chirho];
    if (tok_chirho.startsWith("--")) {
      const key_chirho = tok_chirho.slice(2);
      const next_chirho = argv_chirho[i_chirho + 1];
      if (next_chirho === undefined || next_chirho.startsWith("--")) {
        out_chirho[key_chirho] = "true";
      } else {
        out_chirho[key_chirho] = next_chirho;
        i_chirho++;
      }
    }
  }
  return out_chirho;
}

function now_iso_chirho(): string {
  return new Date().toISOString().replace("T", " ").slice(0, 19);
}

function die_chirho(msg_chirho: string): never {
  console.error(`log_step_chirho: ${msg_chirho}`);
  process.exit(1);
}

const [, , cmd_chirho, ...rest_chirho] = process.argv;
const args_chirho = parse_args_chirho(rest_chirho);
const db_chirho = new Database(DB_PATH_CHIRHO);

switch (cmd_chirho) {
  case "start": {
    const agent_chirho = args_chirho.agent ?? die_chirho("start needs --agent <code>");
    const action_chirho = args_chirho.action ?? die_chirho("start needs --action \"...\"");
    const stmt_chirho = db_chirho.query(
      `INSERT INTO steps_taken_chirho
         (agent_code_chirho, timestamp_start_chirho, action_taken_chirho)
       VALUES (?, ?, ?)
       RETURNING id_chirho`,
    );
    const row_chirho = stmt_chirho.get(agent_chirho, now_iso_chirho(), action_chirho) as {
      id_chirho: number;
    };
    console.log(row_chirho.id_chirho);
    break;
  }

  case "end": {
    const id_chirho = args_chirho.id ?? die_chirho("end needs --id <n>");
    const result_chirho = args_chirho.result ?? die_chirho("end needs --result \"...\"");
    const overview_chirho = args_chirho.overview ?? "";
    const stmt_chirho = db_chirho.query(
      `UPDATE steps_taken_chirho
          SET timestamp_end_chirho = ?, result_of_action_chirho = ?, overview_of_result_chirho = ?
        WHERE id_chirho = ?`,
    );
    stmt_chirho.run(now_iso_chirho(), result_chirho, overview_chirho, Number(id_chirho));
    const check_chirho = db_chirho
      .query(`SELECT id_chirho FROM steps_taken_chirho WHERE id_chirho = ?`)
      .get(Number(id_chirho));
    if (!check_chirho) die_chirho(`no such id_chirho ${id_chirho}`);
    console.log(`closed ${id_chirho}`);
    break;
  }

  case "list": {
    const limit_chirho = Number(args_chirho.limit ?? "10");
    const rows_chirho = db_chirho
      .query(
        `SELECT id_chirho, agent_code_chirho, timestamp_start_chirho, action_taken_chirho
           FROM steps_taken_chirho ORDER BY id_chirho DESC LIMIT ?`,
      )
      .all(limit_chirho) as Array<Record<string, string>>;
    for (const r_chirho of rows_chirho) {
      console.log(
        `${r_chirho.id_chirho}\t${r_chirho.agent_code_chirho}\t${r_chirho.timestamp_start_chirho}\t${String(
          r_chirho.action_taken_chirho,
        ).slice(0, 80)}`,
      );
    }
    break;
  }

  default:
    die_chirho("usage: log_step_chirho.ts <start|end|list> [--flags]");
}

db_chirho.close();
