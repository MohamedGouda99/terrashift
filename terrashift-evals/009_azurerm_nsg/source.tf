resource "aws_security_group" "web" {
  name        = "allow-web"
  description = "Allow inbound web traffic"
  vpc_id      = aws_vpc.main.id
}
