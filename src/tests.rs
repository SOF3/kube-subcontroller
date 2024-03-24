use k8s_openapi::api::core::v1::Pod;

use crate::subscriber;

async fn _test_types() {
    let client = kube::Client::try_default().await.unwrap();

    crate::run(crate::on({
        let client = client.clone();
        move || subscriber::types(client.clone())
    }))
    .await
    .unwrap();
}

async fn _test_objects() {
    let client = kube::Client::try_default().await.unwrap();
    let api = kube::Api::<Pod>::all(client);

    crate::run(crate::on({
        move || subscriber::objects(api.clone(), kube_runtime::watcher::Config::default())
    }))
    .await
    .unwrap();
}
