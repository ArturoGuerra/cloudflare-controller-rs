use crate::crd::credentials::{self, Credentials as CredentialsCrd};
use cloudflare::framework::auth::Credentials;

pub struct Auth {
    pub account_id: String,
    pub kind: Credentials,
}

impl From<CredentialsCrd> for Auth {
    fn from(s: CredentialsCrd) -> Auth {
        let account_id = s.spec.account_id;
        let kind = match s.spec.auth {
            credentials::AuthKind::ServiceKey(key) => Credentials::Service { key },
            credentials::AuthKind::UserAuthKey { email, key } => {
                Credentials::UserAuthKey { email, key }
            }
            credentials::AuthKind::UserAuthToken(token) => Credentials::UserAuthToken { token },
        };

        Auth { account_id, kind }
    }
}
