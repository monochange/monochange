//! Projects glance using the same account as the website.
use leptoskit::prelude::*;
use monochange_shared::ClientSession;
use monochange_shared::OrganizationSummary;

use crate::session;
use crate::theme;
use crate::ui;

#[derive(Clone)]
enum State {
	Loading,
	SignedOut,
	Ready(Vec<OrganizationSummary>),
	Detail(monochange_shared::ProjectDetails),
	Failed(String),
}

/// Session state survives every native tree rebuild.
#[derive(Clone, Copy)]
pub struct HomeModel {
	credential: RwSignal<Option<ClientSession>>,
	state: RwSignal<State>,
}
impl HomeModel {
	pub fn restore() -> Self {
		let credential = RwSignal::new(None::<ClientSession>);
		let state = RwSignal::new(State::Loading);
		crate::tasks::spawn(async move {
			match session::restore().await {
				Ok(saved) => {
					credential.set(saved);
					refresh(credential, state);
				}
				Err(error) => state.set(State::Failed(error)),
			}
		});
		Self { credential, state }
	}
}

#[component]
pub fn Home(model: HomeModel) -> Element {
	let HomeModel { credential, state } = model;
	view! { <View class=format!("p-6 {}", theme::PAGE)><View class=theme::GAP_ACTION>
						<ui::Title text="monochange projects" />
			{move || credential.get().map(|saved| view! { <ui::Quiet text=format!("Signed in as {}", saved.user.github_login) /> }).unwrap_or_else(|| view! { <View /> })}<ui::Quiet text="The products you ship, together." />
						{move || match state.get() {
							State::Loading => view! { <ui::Loading what="projects" /> },
							State::SignedOut => view! { <ui::Quiet text="Sign in with your monochange account." /> },
							State::Failed(error) => view! { <ui::Problem message=&error /> },
							State::Detail(project) => view! { <ui::Panel><ui::Section text=project.project.name /><ui::Quiet text=project.project.description /><ui::Quiet text=format!("Manage this project at {}{}", crate::SERVER, monochange_shared::project_path(&project.organization, &project.project.slug)) />{project.repositories.into_iter().map(|repo| view! { <ui::Quiet text=repo.full_name /> }).collect::<Fragment>()}</ui::Panel> },
				State::Ready(orgs) if orgs.is_empty() => view! { <ui::Quiet text="No organizations yet. Connect repositories on the website to get started." /> },
							State::Ready(orgs) => view! { <View>{orgs.into_iter().map(|org| view! { <ui::Panel><ui::Section text=org.login.clone() />{org.projects.into_iter().map(|project| view! { <View><ui::Section text=project.name /><ui::Quiet text=project.description /><ui::Secondary label="Open project" on_press={let organization = org.login.clone(); let slug = project.slug; move || { let organization=organization.clone();let slug=slug.clone(); state.set(State::Loading);crate::tasks::spawn(async move { let saved = credential.get();match session::client(saved.as_ref()).project(&organization,&slug).await { Ok(Some(project)) => state.set(State::Detail(project)), Ok(None) => state.set(State::Failed("Project not found".into())), Err(error) => handle_error(credential, state, error).await } }); }} /></View> }).collect::<Fragment>()}</ui::Panel> }).collect::<Fragment>()}</View> },
						}}
						{move || if credential.get().is_none() && !matches!(state.get(), State::Loading) { view! { <ui::Primary label="Sign in" enabled=true on_press=move || {
							state.set(State::Loading);
							crate::tasks::spawn(async move { match session::sign_in().await { Ok(saved) => { credential.set(Some(saved)); refresh(credential,state); }, Err(error) => state.set(State::Failed(error)) } });
						} /> }.into() } else { view! { <View /> } }}
						<ui::Secondary label="Manage projects on the website" on_press=move || { crate::tasks::spawn(async move { if let Err(error) = session::open_projects().await { state.set(State::Failed(error)); } }); } />
		{move || if credential.get().is_some() { view! { <View><ui::Secondary label="Refresh projects" on_press=move || refresh(credential,state) />
						<ui::Secondary label="Sign out" on_press=move || {
							state.set(State::Loading);
							crate::tasks::spawn(async move {
								let saved = credential.get();
								let revoked = if let Some(saved) = saved.as_ref() { session::client(Some(saved)).revoke(&saved.access_token).await.map_err(|e| e.to_string()) } else { Ok(()) };
								match session::clear().await { Ok(()) => { credential.set(None); state.set(match revoked { Ok(()) => State::SignedOut, Err(error) => State::Failed(format!("Signed out on this device; remote revocation failed: {error}")) }); }, Err(error) => state.set(State::Failed(error)) }
							});
						} />
	</View> }.into() } else { view! { <View /> } }}
					</View></View> }
}
fn refresh(credential: RwSignal<Option<ClientSession>>, state: RwSignal<State>) {
	let Some(saved) = credential.get() else {
		state.set(State::SignedOut);
		return;
	};
	state.set(State::Loading);
	crate::tasks::spawn(async move {
		match session::client(Some(&saved)).organizations().await {
			Ok(orgs) => state.set(State::Ready(orgs)),
			Err(monochange_shared::api::ApiError::Status { status: 401, .. }) => {
				match session::clear().await {
					Ok(()) => {
						credential.set(None);
						state.set(State::SignedOut);
					}
					Err(error) => state.set(State::Failed(error)),
				}
			}
			Err(error) => state.set(State::Failed(error.to_string())),
		}
	});
}

async fn handle_error(
	credential: RwSignal<Option<ClientSession>>,
	state: RwSignal<State>,
	error: monochange_shared::ApiError,
) {
	if matches!(
		error,
		monochange_shared::ApiError::Status { status: 401, .. }
	) {
		match session::clear().await {
			Ok(()) => {
				credential.set(None);
				state.set(State::SignedOut);
			}
			Err(error) => state.set(State::Failed(error)),
		}
	} else {
		state.set(State::Failed(error.to_string()));
	}
}

#[cfg(test)]
#[path = "__tests__/home_tests.rs"]
mod tests;
