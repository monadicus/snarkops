#[tarpc::service]
pub trait NodeService {
    async fn foo();
}
