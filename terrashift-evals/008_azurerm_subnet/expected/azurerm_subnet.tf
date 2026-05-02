resource "azurerm_subnet" "private" {
  name = "private-subnet"
  resource_group_name = azurerm_resource_group.main.name
  virtual_network_name = azurerm_virtual_network.main.name
}
