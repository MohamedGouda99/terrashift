resource "azurerm_virtual_network" "main" {
  name                = "main"
  resource_group_name = "main-rg"
  location            = "East US"
  address_space       = ["10.0.0.0/16"]
}
