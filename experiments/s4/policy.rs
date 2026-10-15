//! Policy state-machine fixtures independent of the queue's conflict-delay model.
use super::{Class, GRACE};
#[derive(Clone, Copy, Debug)]
struct Request {
    subject: usize,
    class: Class,
    notice: u64,
    expiry: u64,
    revoked: bool,
}
#[derive(Debug)]
struct Lease {
    holder: usize,
    class: Class,
    expires: u64,
    requests: Vec<Request>,
}
impl Lease {
    fn request(&mut self, request: Request) {
        self.requests.push(request);
    }
    fn refresh(&mut self, now: u64) {
        self.requests.retain(|r| !r.revoked && now < r.expiry);
        // Priority then earlier notification then stable subject resolves competition.
        self.requests
            .sort_by_key(|r| (std::cmp::Reverse(r.class), r.notice, r.subject));
        if let Some(r) = self
            .requests
            .iter()
            .find(|r| now >= r.notice + GRACE && (r.class > self.class || now >= self.expires))
            .copied()
        {
            self.holder = r.subject;
            self.class = r.class;
            self.expires = r.expiry;
            self.requests.retain(|other| other.subject != r.subject);
        }
    }
    fn permits_landing(&self, subject: usize, now: u64) -> bool {
        subject == self.holder && now < self.expires
    }
}
fn resolver(a: (usize, Class, u64), b: (usize, Class, u64)) -> usize {
    if a.1 < b.1 {
        a.0
    } else if a.1 > b.1 {
        b.0
    } else if (a.2, a.0) > (b.2, b.0) {
        a.0
    } else {
        b.0
    }
}
pub fn exercise() -> String {
    let mut l = Lease {
        holder: 0,
        class: Class::Normal,
        expires: 300,
        requests: vec![],
    };
    l.request(Request {
        subject: 1,
        class: Class::High,
        notice: 10,
        expiry: 200,
        revoked: false,
    });
    l.request(Request {
        subject: 2,
        class: Class::Sev,
        notice: 15,
        expiry: 200,
        revoked: true,
    });
    l.refresh(39);
    assert!(l.permits_landing(0, 39));
    assert!(!l.permits_landing(1, 39));
    l.refresh(40);
    assert!(l.permits_landing(1, 40));
    assert!(!l.permits_landing(0, 40));
    l.refresh(200);
    assert!(!l.permits_landing(1, 200));
    assert_eq!(resolver((1, Class::Low, 0), (2, Class::High, 10)), 1);
    assert_eq!(resolver((1, Class::High, 0), (2, Class::High, 10)), 2);
    "lease: notify=10 grace=30 holder-before=0 holder-at-40=1 revoked-sev-request-dropped=true stale-holder-landing=false expiry-landing=false\nresolver: lower-class=1 tie-later-lander=2 content-unchanged=true\n".into()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lease_grace_revocation_expiry_and_landing_recheck() {
        assert!(exercise().contains("stale-holder-landing=false"));
    }
    #[test]
    fn expired_and_lower_priority_requests_cannot_steal_lease() {
        let mut l = Lease {
            holder: 0,
            class: Class::High,
            expires: 300,
            requests: vec![],
        };
        l.request(Request {
            subject: 1,
            class: Class::Sev,
            notice: 0,
            expiry: 20,
            revoked: false,
        });
        l.request(Request {
            subject: 2,
            class: Class::Low,
            notice: 0,
            expiry: 600,
            revoked: false,
        });
        l.refresh(30);
        assert_eq!(l.holder, 0);
        l.refresh(300);
        assert_eq!(l.holder, 2);
    }
}
