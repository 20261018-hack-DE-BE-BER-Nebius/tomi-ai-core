use crate::{
    AppState,
    api::{
        error::ApiError,
        types::{ChatInput, ClientMessage, ContextMessage, TraceEvent},
    },
    application::chat,
    domain::policy,
};
use axum::{
    Json,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use serde_json::{Value, json};
use tokio::{sync::mpsc, task::JoinHandle};

pub async fn upgrade(State(state): State<AppState>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(131072)
        .on_upgrade(move |socket| connection(socket, state))
}

struct Active {
    run: String,
    input: ChatInput,
    task: JoinHandle<()>,
}
impl Drop for Active {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn connection(socket: WebSocket, state: AppState) {
    let (mut sink, mut incoming) = socket.split();
    let (tx, mut events) = mpsc::channel::<TraceEvent>(128);
    let mut active: Option<Active> = None;
    let mut history = crate::application::history::SessionHistory::default();
    loop {
        tokio::select! {
            message=incoming.next()=>{
                let text=match message { Some(Ok(Message::Text(text)))=>text, Some(Ok(Message::Ping(_) | Message::Pong(_)))=>continue, _=>break };
                match serde_json::from_str::<ClientMessage>(&text){
                    Ok(message)=>handle(message,&state,&tx,&mut active,&history).await,
                    Err(_)=>{let event=TraceEvent::new("run.failed","","",json!({"code":"invalid_request","message":"Invalid WebSocket command"}));let _=tx.send(event).await;}
                }
            },
            event=events.recv()=>{
                let Some(event)=event else{break;};
                if active.is_none() && event.kind!="run.cancelled" && !event.run_id.is_empty(){continue;}
                if let Some(current)=&active{
                    if event.run_id!=current.run&&!event.run_id.is_empty(){continue;}
                    if event.kind=="answer.completed"{
                        history.remember(&current.input.session_id,&current.input.text,event.data["summary"].as_str().unwrap_or(""));
                    }
                }
                if sink.send(Message::Text(event_to_string(&event).into())).await.is_err(){break;}
            }
        }
    }
}

fn event_to_string(event: &TraceEvent) -> String {
    serde_json::to_string(event).expect("event serializes")
}

async fn handle(
    message: ClientMessage,
    state: &AppState,
    tx: &mpsc::Sender<TraceEvent>,
    active: &mut Option<Active>,
    history: &crate::application::history::SessionHistory,
) {
    match message {
        ClientMessage::Message { mut input } => {
            if let Some(old) = active.take() {
                let _ = chat::emit(
                    tx,
                    "run.cancelled",
                    &old.run,
                    &old.input,
                    json!({"reason":"superseded"}),
                )
                .await;
            }
            if input.context.is_empty() {
                input.context = history.get(&input.session_id);
            }
            start(state, tx, active, input, None).await;
        }
        ClientMessage::Steer { run_id, text } => {
            if !policy::validate_text(&text) {
                return;
            }
            if active.as_ref().is_some_and(|a| a.run == run_id) {
                let old = active.take().unwrap();
                let mut input = old.input.clone();
                input.context.push(ContextMessage {
                    role: "user".into(),
                    content: input.text.clone(),
                });
                input.text = text;
                let successor = uuid::Uuid::new_v4().to_string();
                let _ = chat::emit(
                    tx,
                    "run.steered",
                    &successor,
                    &input,
                    json!({"previous_run_id":run_id,"strategy":"restart_with_preserved_context"}),
                )
                .await;
                drop(old);
                start(state, tx, active, input, Some(successor)).await;
            }
        }
        ClientMessage::Cancel { run_id } => {
            if active.as_ref().is_some_and(|a| a.run == run_id) {
                let old = active.take().unwrap();
                let _ = chat::emit(
                    tx,
                    "run.cancelled",
                    &run_id,
                    &old.input,
                    json!({"reason":"user"}),
                )
                .await;
            }
        }
        ClientMessage::Ping => {
            let _ = tx.send(TraceEvent::new("pong", "", "", json!({}))).await;
        }
    }
}

async fn start(
    state: &AppState,
    tx: &mpsc::Sender<TraceEvent>,
    active: &mut Option<Active>,
    input: ChatInput,
    existing: Option<String>,
) {
    let run = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let task = tokio::spawn(chat::run(
        state.clone(),
        input.clone(),
        run.clone(),
        tx.clone(),
    ));
    *active = Some(Active { run, input, task });
}

pub async fn compare(
    State(state): State<AppState>,
    Json(input): Json<ChatInput>,
) -> Result<Json<Value>, ApiError> {
    chat::validate(&input)?;
    let (tx, mut rx) = mpsc::channel(128);
    let run = uuid::Uuid::new_v4().to_string();
    let task = tokio::spawn(chat::run(state, input, run, tx));
    let _guard = crate::infrastructure::task::AbortOnDrop(task.abort_handle());
    let mut traces = Vec::new();
    let mut answer = None;
    while let Some(event) = rx.recv().await {
        if event.kind == "answer.completed" {
            answer = Some(event.data.clone());
        }
        if event.kind == "run.failed" {
            return Err(ApiError::unavailable(
                event.data["message"]
                    .as_str()
                    .unwrap_or("Comparison failed"),
            ));
        }
        traces.push(event);
    }
    task.await.map_err(|_| ApiError::transport())?;
    Ok(Json(
        json!({"answer":answer.ok_or_else(ApiError::transport)?,"traces":traces}),
    ))
}
