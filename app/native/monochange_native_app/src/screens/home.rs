//! Native home: auth handshake + project floor (Release floor product cut).

use leptoskit::prelude::*;
use monochange_rpc::AuthHandshake;
use monochange_rpc::ListProjects;
use monochange_rpc::ProjectSummary;
use monochange_rpc::SessionProbe;

use crate::theme;
use crate::ui;

#[derive(Clone, Debug, Default)]
enum Load<T> {
	#[default]
	Idle,
	Loading,
	Ready(T),
	Failed(String),
}

/// Workspace shell: handshake status, then the projects list.
#[component]
pub fn Home() -> Element {
	let handshake = RwSignal::new(Load::<AuthHandshake>::Idle);
	let projects = RwSignal::new(Load::<Vec<ProjectSummary>>::Idle);

	view! {
		<View class=format!("p-6 {}", theme::PAGE)>
			<View class=theme::GAP_ACTION>
				<ui::Title text="monochange" />
				<ui::Quiet text="Ship every package from one floor." />
				{move || {
					view! { <HandshakeStatus state=handshake.get() /> }
				}}
				{move || {
					view! { <ProjectFloor state=projects.get() /> }
				}}
				<View class=format!("flex-row flex-wrap {}", theme::GAP_ACTION)>
					<ui::Primary
						label="Refresh handshake"
						enabled=true
						on_press=move || refresh_handshake(handshake, projects)
					/>
					<ui::Secondary
						label="Reload projects"
						on_press=move || refresh_projects(projects)
					/>
				</View>
			</View>
		</View>
	}
}

/// Auth handshake panel for the current load state.
#[component]
fn HandshakeStatus(state: Load<AuthHandshake>) -> Element {
	match state {
		Load::Idle => {
			view! {
				<ui::Quiet text="Tap refresh to run the auth handshake over leptoskit_rpc." />
			}
		}
		Load::Loading => view! { <ui::Loading what="session" /> },
		Load::Failed(message) => view! { <ui::Problem message=&message /> },
		Load::Ready(handshake) => {
			let detail = match &handshake.session {
				Some(user) => {
					format!(
						"Signed in as @{} · plan {} · {}",
						user.github_login, user.plan_tier, handshake.protocol
					)
				}
				None => format!("Anonymous · {}", handshake.protocol),
			};
			let status = handshake.status;
			view! {
				<ui::Panel>
					<ui::Section text="Auth handshake" />
					<Text class=theme::SIGNAL>{status}</Text>
					<ui::Quiet text=detail />
				</ui::Panel>
			}
		}
	}
}

/// Project floor list for the current load state.
#[component]
fn ProjectFloor(state: Load<Vec<ProjectSummary>>) -> Element {
	match state {
		Load::Idle => view! { <ui::Quiet text="Projects load after the handshake." /> },
		Load::Loading => view! { <ui::Loading what="projects" /> },
		Load::Failed(message) => view! { <ui::Problem message=&message /> },
		Load::Ready(list) if list.is_empty() => {
			view! {
				<ui::Quiet text="No projects yet. Connect repositories on the web workspace." />
			}
		}
		Load::Ready(list) => {
			view! {
				<View class=theme::GAP_ACTION>
					<ui::Section text="Projects" />
					{list
						.into_iter()
						.map(|project| view! { <ProjectRow project=project /> })
						.collect::<Fragment>()}
				</View>
			}
		}
	}
}

/// One project row on the floor.
#[component]
fn ProjectRow(project: ProjectSummary) -> Element {
	let meta = format!(
		"{}/{} · {} repos",
		project.organization, project.slug, project.repository_count
	);
	view! {
		<ui::Panel>
			<Text class=format!("text-lg font-bold {}", theme::INK)>{project.name}</Text>
			<ui::Quiet text=project.description />
			<ui::Quiet text=meta />
		</ui::Panel>
	}
}

fn refresh_handshake(
	handshake: RwSignal<Load<AuthHandshake>>,
	projects: RwSignal<Load<Vec<ProjectSummary>>>,
) {
	handshake.set(Load::Loading);
	crate::tasks::spawn(async move {
		match leptoskit::rpc::client::call(SessionProbe {}).await {
			Ok(result) => {
				handshake.set(Load::Ready(result));
				refresh_projects(projects);
			}
			Err(error) => handshake.set(Load::Failed(error.to_string())),
		}
	});
}

fn refresh_projects(projects: RwSignal<Load<Vec<ProjectSummary>>>) {
	projects.set(Load::Loading);
	crate::tasks::spawn(async move {
		match leptoskit::rpc::client::call(ListProjects {}).await {
			Ok(result) => projects.set(Load::Ready(result)),
			Err(error) => projects.set(Load::Failed(error.to_string())),
		}
	});
}
