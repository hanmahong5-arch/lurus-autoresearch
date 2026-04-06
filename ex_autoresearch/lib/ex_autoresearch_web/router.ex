defmodule ExAutoresearchWeb.Router do
  use ExAutoresearchWeb, :router

  import Oban.Web.Router
  import Phoenix.LiveDashboard.Router

  pipeline :browser do
    plug :accepts, ["html"]
    plug :fetch_session
    plug :fetch_live_flash
    plug :put_root_layout, html: {ExAutoresearchWeb.Layouts, :root}
    plug :protect_from_forgery
    plug :put_secure_browser_headers
    plug ExAutoresearchWeb.Plugs.Auth, :fetch_current_user
    plug ExAutoresearchWeb.Plugs.SetAssignments
  end

  pipeline :require_auth do
    plug ExAutoresearchWeb.Plugs.Auth, :require_authenticated_user
  end

  pipeline :redirect_if_auth do
    plug ExAutoresearchWeb.Plugs.Auth, :redirect_if_user_is_authenticated
  end

  pipeline :api do
    plug :accepts, ["json"]
  end

  # Public auth routes
  scope "/", ExAutoresearchWeb do
    pipe_through [:browser, :redirect_if_auth]

    live "/login", AuthLive
    post "/session", SessionController, :create
    post "/register", SessionController, :create_registration
    get "/logout", SessionController, :delete
  end

  # Protected app routes
  scope "/", ExAutoresearchWeb do
    pipe_through [:browser, :require_auth]

    live "/", DashboardLive, :index
    live "/dashboard", DashboardLive, :index
    live "/templates", TemplateLive, :index
    live "/templates/:id", TemplateLive, :show
    live "/schedules", ScheduleLive, :index
    live "/settings", SettingsLive, :index
    live "/reports/:id", ReportDetailLive, :show
  end

  # Dev routes
  if Application.compile_env(:ex_autoresearch, :dev_routes) do
    scope "/dev" do
      pipe_through :browser

      live_dashboard "/dashboard", metrics: ExAutoresearchWeb.Telemetry
      forward "/mailbox", Plug.Swoosh.MailboxPreview
    end

    scope "/" do
      pipe_through :browser

      oban_dashboard("/oban")
    end
  end
end
