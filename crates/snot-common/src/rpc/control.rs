#[tarpc::service]
pub trait ControlService {
    async fn placeholder() -> String;
}
