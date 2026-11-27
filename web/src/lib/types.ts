export interface User {
  id: string;
  email: string | null;
  full_name: string | null;
  avatar_url: string | null;
  timezone: string;
  coin_balance: number;
  role?: "user" | "subadmin" | "admin";
  email_verified?: boolean;
  has_premium?: boolean;
}

export interface AuthResponse {
  access_token: string;
  expires_in: number;
  user: User;
}

export interface RefreshResponse {
  access_token: string;
  expires_in: number;
}

export interface MessageResponse {
  message: string;
}

export interface ApiErrorResponse {
  error: {
    code: string;
    message: string;
  };
}
