use actix_web::{get, http::header, post, web, HttpResponse, Result};
use miden_client::{
    client::transactions::transaction_request::TransactionTemplate, store::InputNoteRecord,
};
use miden_objects::{
    accounts::AccountId,
    assets::FungibleAsset,
    notes::{NoteId, NoteType},
    utils::serde::Serializable,
};
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::{errors::FaucetError, utils::FaucetState};

#[derive(Deserialize)]
struct FaucetRequest {
    account_id: String,
}

#[derive(Serialize)]
struct FaucetMetadataReponse {
    id: String,
    asset_amount: u64,
}

#[get("/get_metadata")]
pub async fn get_metadata(state: web::Data<FaucetState>) -> HttpResponse {
    let response = FaucetMetadataReponse {
        id: state.id.to_string(),
        asset_amount: state.asset_amount,
    };

    HttpResponse::Ok().json(response)
}

#[post("/get_tokens")]
pub async fn get_tokens(
    req: web::Json<FaucetRequest>,
    state: web::Data<FaucetState>,
) -> Result<HttpResponse> {
    info!("Received a request with account_id: {}", req.account_id);

    let client = state.client.clone();

    // Receive and hex user account id
    let target_account_id = AccountId::from_hex(req.account_id.as_str())
        .map_err(|err| FaucetError::BadRequest(err.to_string()))?;

    // Instantiate asset
    let asset = FungibleAsset::new(state.id, state.asset_amount)
        .map_err(|err| FaucetError::InternalServerError(err.to_string()))?;

    // Instantiate note type
    let note_type = NoteType::OffChain;

    // Instantiate transaction template
    let tx_template = TransactionTemplate::MintFungibleAsset(asset, target_account_id, note_type);

    // Instantiate transaction request
    let tx_request = client
        .lock()
        .await
        .build_transaction_request(tx_template)
        .map_err(|err| FaucetError::InternalServerError(err.to_string()))?;

    // Run transaction executor & execute transaction
    let tx_result = client
        .lock()
        .await
        .new_transaction(tx_request)
        .map_err(|err| FaucetError::InternalServerError(err.to_string()))?;

    // Get created notes from transaction result
    let created_notes = tx_result.created_notes().clone();

    // Run transaction prover & send transaction to node
    client
        .lock()
        .await
        .submit_transaction(tx_result)
        .await
        .map_err(|err| FaucetError::InternalServerError(err.to_string()))?;

    let note_id: NoteId;

    // Serialize note into bytes
    let bytes = match created_notes.first() {
        Some(note) => {
            note_id = note.id();
            InputNoteRecord::from(note.clone()).to_bytes()
        },
        None => {
            return Err(
                FaucetError::InternalServerError("Failed to generate note.".to_string()).into()
            )
        },
    };

    info!("A new note has been created: {}", note_id);

    // Send generated note to user
    Ok(HttpResponse::Ok()
        .content_type("application/octet-stream")
        .append_header(header::ContentDisposition {
            disposition: actix_web::http::header::DispositionType::Attachment,
            parameters: vec![actix_web::http::header::DispositionParam::Filename(
                "note.mno".to_string(),
            )],
        })
        .body(bytes))
}
