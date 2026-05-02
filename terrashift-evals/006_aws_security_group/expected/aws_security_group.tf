resource "aws_security_group" "web" {
  name = "allow-web"
  vpc_id = aws_vpc.main.id
  description = "Allow inbound web traffic on 80 and 443"
}
