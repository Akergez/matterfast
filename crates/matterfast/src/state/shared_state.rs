use std::cell::RefCell;
use std::rc::Rc;

use super::app_state::AppState;

pub type SharedState = Rc<RefCell<AppState>>;
