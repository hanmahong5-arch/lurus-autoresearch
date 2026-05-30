defmodule ExAutoresearch.Research do
  @moduledoc """
  Ash domain combining Research, Accounts, and Template resources.
  """

  use Ash.Domain

  resources do
    resource ExAutoresearch.Research.Brief
    resource ExAutoresearch.Research.Report
    resource ExAutoresearch.Research.Investigation
    resource ExAutoresearch.Research.Template
    resource ExAutoresearch.Research.Source
    resource ExAutoresearch.Research.Claim
    resource ExAutoresearch.Research.Delta

    resource ExAutoresearch.Accounts.User
    resource ExAutoresearch.Accounts.Organization
    resource ExAutoresearch.Accounts.Membership
  end
end
