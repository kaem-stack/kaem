use tui_input::InputRequest;

pub enum Action {
    Quit,
    NextContact,
    PreviousContact,
    SendMessage,
    Input(InputRequest),
}
