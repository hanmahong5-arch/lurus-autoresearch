defmodule ExAutoresearch.Accounts do
  @moduledoc """
  Convenience module for accessing account-related resources.
  """

  alias ExAutoresearch.Accounts.{User, Organization, Membership}

  def get_user(id), do: Ash.get(User, id)

  def get_user_by_email(_email), do: {:error, :not_implemented}

  @doc """
  Find a user with the given email.
  """
  def find_user_by_email(email) do
    case Ash.read(User) do
      {:ok, users} ->
        case Enum.find(users, fn u -> u.email == email end) do
          nil -> {:error, :not_found}
          user -> {:ok, user}
        end
      {:error, reason} -> {:error, reason}
    end
  end

  @doc """
  List all organizations for a given user.
  """
  def list_user_organizations(user_id) do
    case Ash.read(Membership) do
      {:ok, memberships} ->
        memberships
        |> Enum.filter(fn m -> m.user_id == user_id end)
        |> Enum.map(fn m ->
          case Ash.load(m, :organization) do
            {:ok, loaded} -> loaded.organization
            _ -> nil
          end
        end)
        |> Enum.reject(&is_nil/1)

      {:error, _} -> []
    end
  end

  @doc """
  Check if a user has membership in an organization.
  """
  def get_membership(user_id, organization_id) do
    case Ash.read(Membership) do
      {:ok, memberships} ->
        case Enum.find(memberships, fn m ->
             m.user_id == user_id and m.organization_id == organization_id
           end) do
          nil -> {:error, :not_found}
          membership -> {:ok, membership}
        end
      {:error, reason} -> {:error, reason}
    end
  end

  def create_organization(name, user_id) do
    with {:ok, org} <- Ash.create(Organization, %{name: name, plan: :free}, action: :create),
         {:ok, _membership} <-
           Ash.create(Membership, %{
             user_id: user_id,
             organization_id: org.id,
             role: :owner
           }, action: :create) do
      {:ok, org}
    end
  end
end
