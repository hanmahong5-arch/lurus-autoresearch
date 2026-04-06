defmodule ExAutoresearch.Research do
  @moduledoc """
  Ash domain combining Research, Accounts, and Template resources.
  """

  use Ash.Domain

  resources do
    resource ExAutoresearch.Research.Report
    resource ExAutoresearch.Research.Investigation
    resource ExAutoresearch.Research.Template

    resource ExAutoresearch.Accounts.User
    resource ExAutoresearch.Accounts.Organization
    resource ExAutoresearch.Accounts.Membership
  end
end
