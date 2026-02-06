use std::fs;
use std::path::{Path, PathBuf};

use blake3::Hasher;
use ed25519_dalek::Keypair;
use rand::rngs::OsRng;

use crate::{CoreError, PeerId};

#[derive(Debug, Clone)]
pub struct Identity {
    pub keypair: Keypair,
    pub peer_id: PeerId,
}

impl Identity {
    pub fn generate() -> Self {
        let mut rng = OsRng;
        let keypair = Keypair::generate(&mut rng);
        let peer_id = peer_id_from_keypair(&keypair);
        Self { keypair, peer_id }
    }

    pub fn default_path() -> Result<PathBuf, CoreError> {
        let base = dirs::config_dir().ok_or(CoreError::ConfigDirMissing)?;
        Ok(base.join("rift").join("identity.key"))
    }

    pub fn load(path: Option<&Path>) -> Result<Self, CoreError> {
        let path = match path {
            Some(path) => path.to_path_buf(),
            None => Self::default_path()?,
        };
        let bytes = fs::read(&path)
            .map_err(|err| if err.kind() == std::io::ErrorKind::NotFound {
                CoreError::IdentityMissing(path.display().to_string())
            } else {
                CoreError::Io(err)
            })?;

        if bytes.len() != 64 {
            return Err(CoreError::InvalidKeyLength);
        }
        let keypair = Keypair::from_bytes(&bytes).map_err(|_| CoreError::InvalidKeyLength)?;
        let peer_id = peer_id_from_keypair(&keypair);
        Ok(Self { keypair, peer_id })
    }

    pub fn load_or_generate(path: Option<&Path>) -> Result<(Self, bool), CoreError> {
        let path = match path {
            Some(path) => path.to_path_buf(),
            None => Self::default_path()?,
        };
        match Self::load(Some(&path)) {
            Ok(identity) => Ok((identity, false)),
            Err(CoreError::IdentityMissing(_)) => {
                let identity = Self::generate();
                identity.save(&path)?;
                Ok((identity, true))
            }
            Err(err) => Err(err),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), CoreError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let bytes = self.keypair.to_bytes();
        fs::write(path, &bytes)?;
        Ok(())
    }
}

fn peer_id_from_keypair(keypair: &Keypair) -> PeerId {
    let mut hasher = Hasher::new();
    hasher.update(keypair.public.as_bytes());
    let hash = hasher.finalize();
    PeerId(*hash.as_bytes())
}
