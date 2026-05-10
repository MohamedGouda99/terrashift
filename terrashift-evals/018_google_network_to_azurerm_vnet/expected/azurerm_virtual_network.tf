resource "azurerm_virtual_network" "main" {
  name = "main"
  resource_group_name = "main-rg"
  location = "East US"
}
