defmodule ExAutoresearch.Accounts.Auth do
  @moduledoc """
  Authentication utilities: register, login, session management.
  """

  alias ExAutoresearch.Accounts

  require Ash.Query

  @doc """
  Register a new user and create the user's first organization.
  """
  def register(email, password, name \\ nil) when is_binary(email) and is_binary(password) do
    # Check if user already exists
    case Ash.read_one(Ash.Query.filter(Accounts.User, email == ^email)) do
      {:ok, existing_user} when not is_nil(existing_user) ->
        {:error, :email_already_exists}

      _ ->
        {:ok, hash} = Bcrypt.hash_pwd_salt(password)

        params = %{
          email: email,
          password_hash: hash,
          name: name
        }

        case Ash.create(Accounts.User, params, action: :register) do
          {:ok, user} ->
            # Create default organization
            org_params = %{
              name: "#{email}'s Workspace",
              plan: :free
            }

            case Ash.create(Accounts.Organization, org_params, action: :create) do
              {:ok, org} ->
                # Create membership
                Ash.create(Accounts.Membership, %{
                  user_id: user.id,
                  organization_id: org.id,
                  role: :owner
                }, action: :create)

                {:ok, user, org}

              {:error, reason} ->
                {:error, reason}
            end

          {:error, reason} ->
            {:error, reason}
        end
    end
  end

  @doc """
  Authenticate a user by email and password.
  Returns {:ok, user} or {:error, :invalid_credentials}.
  """
  def login(email, password) when is_binary(email) and is_binary(password) do
    case Ash.read_one(Ash.Query.filter(Accounts.User, email == ^email)) do
      {:ok, user} when not is_nil(user) ->
        if Bcrypt.verify_pass(password, user.password_hash) do
          {:ok, user}
        else
          {:error, :invalid_credentials}
        end

      _ ->
        # Use Bcrypt.no_user_verify/0 to prevent timing attacks
        Bcrypt.no_user_verify()
        {:error, :invalid_credentials}
    end
  end

  @doc """
  Get a user by ID.
  """
  def get_user(user_id) do
    Ash.get(Accounts.User, user_id)
  end

  @doc """
  Get all organizations for a user.
  """
  def user_organizations(user_id) do
    Accounts.Membership
    |> Ash.Query.filter(user_id == ^user_id)
    |> Ash.Query.load(:organization)
    |> Ash.read!()
    |> Enum.map(& &1.organization)
  end

  @doc """
  Get a user's membership in a specific organization.
  """
  def membership(user_id, organization_id) do
    Accounts.Membership
    |> Ash.Query.filter(user_id == ^user_id and organization_id == ^organization_id)
    |> Ash.read_one()
  end
end
