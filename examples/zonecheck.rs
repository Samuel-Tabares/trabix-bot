//! Utilitario de un solo uso: pasa las direcciones guardadas por la tabla de
//! zonas nueva y reporta cuales quedaron con una zona distinta a la persistida.
//! NO escribe nada en la base.
use granizado_bot::bot::armenia_zones::{lookup_zone, ZoneLookup};

fn main() {
    let path = std::env::args().nth(1).expect("ruta del archivo");
    let raw = std::fs::read_to_string(path).unwrap();
    for line in raw.lines().filter(|l| !l.trim().is_empty()) {
        let f: Vec<&str> = line.split('|').collect();
        let (id, kind, zone, cost, addr) = (f[0], f[1], f[2], f[3], f[4..].join("|"));
        if kind != "armenia" {
            println!("{id}  SKIP ({kind})  {addr}");
            continue;
        }
        let verdict = match lookup_zone(&addr) {
            ZoneLookup::Resolved { zone: z, barrio } => {
                let now = z.storage_key();
                if now == zone {
                    format!("OK      {now} (${})  [{barrio}]", z.delivery_cost())
                } else {
                    format!(
                        "CAMBIA  {zone} (${cost}) -> {now} (${})  [{barrio}]",
                        z.delivery_cost()
                    )
                }
            }
            ZoneLookup::Ambiguous { barrio } => {
                format!("PREGUNTA  [{barrio}] (guardado: {zone} ${cost})")
            }
            ZoneLookup::Unknown => format!("PREGUNTA  sin barrio conocido (guardado: {zone} ${cost})"),
        };
        println!("{id}  {verdict}  {addr}");
    }
}
