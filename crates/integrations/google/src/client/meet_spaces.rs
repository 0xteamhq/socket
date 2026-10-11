//! Meeting spaces: the place a meeting code or a link leads to.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use super::meet::{SPACE, without_link};
use crate::models::MeetSpace;

/// Meeting spaces.
#[derive(Debug, Clone, Copy)]
pub struct MeetSpaces<'a>(pub(crate) Api<'a>);

impl MeetSpaces<'_> {
    /// Gets a space, with the meeting going on in it if there is one.
    ///
    /// `space` is the space's name (`spaces/{id}`), its id, a meeting code
    /// (`abc-mnop-xyz`), or the link people join by. Google takes a meeting
    /// code in the place of the id, so all of them are asked for the same way.
    pub async fn get(&self, space: &str) -> Result<MeetSpace> {
        let path = self.0.meet(&[(&SPACE, without_link(space))])?;
        let space: MeetSpace = self.0.decode(self.0.send(RawRequest::get(path)).await?, "a space")?;
        if space.name.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a space"));
        }
        Ok(space)
    }
}
