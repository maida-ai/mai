//! Strict, bounded, unquoted CSV workload import. Values are simulated seconds.
use super::*;
pub const HEADER: &str =
    "id,agent,arrival_s,class,dependency,gate_s,fail,flake,conflict,grant_expires_s,grant_revoke_s";
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
#[allow(clippy::manual_is_multiple_of)] // Keep Rust 1.85 compatibility.
pub fn parse(text: &str) -> io::Result<Vec<Job>> {
    let mut lines = text.lines();
    if lines.next() != Some(HEADER) {
        return Err(invalid("unsupported workload CSV header"));
    }
    let mut jobs: Vec<Job> = Vec::new();
    for (row, line) in lines.enumerate() {
        let cells: Vec<_> = line.split(',').collect();
        if cells.len() != 11 || row >= 3000 {
            return Err(invalid("CSV needs 11 columns and at most 3000 rows"));
        }
        let n = |i: usize| -> io::Result<u64> {
            cells[i]
                .parse::<u64>()
                .map_err(|_| invalid("invalid unsigned integer"))
        };
        let optional = |i: usize| -> io::Result<Option<u64>> {
            if cells[i] == "-" {
                Ok(None)
            } else {
                n(i).map(Some)
            }
        };
        let boolean = |i: usize| -> io::Result<bool> {
            match cells[i] {
                "true" => Ok(true),
                "false" => Ok(false),
                _ => Err(invalid("boolean must be true or false")),
            }
        };
        let id = n(0)? as usize;
        let agent = n(1)? as usize;
        let arrival = n(2)?;
        let requested = match cells[3] {
            "low" => Class::Low,
            "normal" => Class::Normal,
            "high" => Class::High,
            "sev" => Class::Sev,
            _ => return Err(invalid("unknown priority")),
        };
        let dep = optional(4)?.map(|n| n as usize);
        let duration = n(5)?;
        if id != jobs.len()
            || agent >= AGENTS
            || arrival > 120000
            || duration == 0
            || duration > 3600
            || arrival % TICK != 0
            || duration % TICK != 0
        {
            return Err(invalid(
                "IDs must be contiguous; agent <30; arrival <=120000; gate 1..3600; times multiples of 10",
            ));
        }
        if dep.is_some_and(|p| p >= id || jobs[p].arrival > arrival) {
            return Err(invalid("dependency must be earlier ID arriving no later"));
        }
        let expiry = optional(9)?;
        let revoked = optional(10)?;
        if requested >= Class::High && expiry.is_none() {
            return Err(invalid("elevated priority requires a scoped grant expiry"));
        }
        if expiry.is_some_and(|t| t <= arrival || t > 240000)
            || revoked.is_some_and(|t| t < arrival || expiry.is_none_or(|e| t > e))
        {
            return Err(invalid("invalid grant validity interval"));
        }
        if requested < Class::High && expiry.is_some() {
            return Err(invalid("low/normal rows must not carry elevated grants"));
        }
        let grant = expiry.map(|expires| Grant {
            subject: id,
            class: requested,
            issued: arrival,
            expires,
            revoked,
        });
        jobs.push(Job {
            id,
            agent,
            arrival,
            requested,
            grant,
            dep,
            duration,
            fails: boolean(6)?,
            flakes: boolean(7)?,
            conflict: boolean(8)?,
            state: State::Waiting,
            aged: false,
            inherited: false,
        });
    }
    if jobs.is_empty() {
        return Err(invalid("empty workload"));
    }
    Ok(jobs)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_rejects_cycles_bad_grants_unknown_schema_and_unbounded_input() {
        let row = "0,0,0,normal,-,60,false,false,false,-,-";
        assert_eq!(parse(&format!("{HEADER}\n{row}\n")).unwrap().len(), 1);
        for bad in [
            row.replace(",-,60", ",0,60"),
            row.replace("normal", "sev"),
            row.replace(",60,", ",6000000,"),
            row.replace(",false,", ",maybe,"),
        ] {
            assert!(parse(&format!("{HEADER}\n{bad}\n")).is_err(), "{bad}");
        }
        assert!(parse("unknown\n").is_err());
    }
}
